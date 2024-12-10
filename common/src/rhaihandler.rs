use std::path::{Path, PathBuf};

use crate::{
    context,
    handlebarshandler::HandleBars,
    hasheshandlers::Argon,
    httphandler::RestClient,
    instancesystem::SystemInstance,
    instancetenant::TenantInstance,
    jukebox::JukeBox,
    k8sgeneric::{update_cache, K8sGeneric, K8sObject},
    k8sworkload::{K8sDaemonSet, K8sDeploy, K8sJob, K8sStatefulSet},
    ocihandler::Registry,
    passwordhandler::Passwords,
    chronohandler::DateTimeHandler,
    rhai_err, shellhandler,
    vynilpackage::{rhai_read_package_yaml, VynilPackageSource},
    Error::{self, *},
    Result, RhaiRes, Semver,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use kube::api::DynamicObject;
pub use rhai::{
    module_resolvers::{FileModuleResolver, ModuleResolversCollection},
    serde::to_dynamic,
    Array, Dynamic, Engine, ImmutableString, Map, Module, Scope,
//    FnPtr, NativeCallContext,
};
use serde::Deserialize;


pub fn base64_decode(input: String) -> Result<String> {
    String::from_utf8(STANDARD.decode(&input).unwrap()).map_err(Error::UTF8)
}

#[derive(Debug)]
pub struct Script {
    pub engine: Engine,
    pub ctx: Scope<'static>,
}

impl Script {
    #[must_use]
    pub fn new(resolver_path: Vec<String>) -> Script {
        let mut script = Script {
            engine: Engine::new(),
            ctx: Scope::new(),
        };

        let mut resolver = ModuleResolversCollection::new();
        for path in resolver_path {
            resolver.push(FileModuleResolver::new_with_path(path));
        }
        script.engine.set_module_resolver(resolver);
        script.engine.set_max_expr_depths(128, 64);
        script
            .engine
            .register_fn("vynil_owner", || -> Dynamic {
                match context::get_owner() {
                    Some(o) => serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap(),
                    None => serde_json::from_str("{}").unwrap(),
                }
            })
            .register_fn("shell_run", shellhandler::rhai_run)
            .register_fn("shell_output", shellhandler::rhai_get_stdout)
            .register_fn("log_debug", |s: ImmutableString| tracing::debug!("{s}"))
            .register_fn("log_info", |s: ImmutableString| tracing::info!("{s}"))
            .register_fn("log_warn", |s: ImmutableString| tracing::warn!("{s}"))
            .register_fn("log_error", |s: ImmutableString| tracing::error!("{s}"))
            .register_fn("gen_password", |len: u32| -> String {
                Passwords::new().generate(len, 6, 2, 2)
            })
            .register_fn("gen_password_alphanum", |len: u32| -> String {
                Passwords::new().generate(len, 8, 2, 0)
            })
            .register_fn("get_env", |var: ImmutableString| -> String {
                std::env::var(var.to_string()).unwrap_or("".into())
            })
            .register_fn(
                "base64_decode",
                |val: ImmutableString| -> RhaiRes<ImmutableString> {
                    base64_decode(val.to_string()).map_err(rhai_err).map(|v| v.into())
                },
            )
            .register_fn("base64_encode", |val: ImmutableString| -> ImmutableString {
                STANDARD.encode(val.to_string()).into()
            })
            .register_fn("json_encode", |val: Dynamic| -> RhaiRes<ImmutableString> {
                serde_json::to_string(&val)
                    .map_err(|e| rhai_err(Error::SerializationError(e)))
                    .map(|v| v.into())
            })
            .register_fn("json_encode_escape", |val: Dynamic| -> RhaiRes<ImmutableString> {
                let str = serde_json::to_string(&val).map_err(|e| rhai_err(Error::SerializationError(e)))?;
                Ok(format!("{:?}", str).into())
            })
            .register_fn("json_decode", |val: ImmutableString| -> RhaiRes<Dynamic> {
                serde_json::from_str(val.as_ref()).map_err(|e| rhai_err(Error::SerializationError(e)))
            })
            .register_fn("yaml_encode", |val: Dynamic| -> RhaiRes<ImmutableString> {
                serde_yaml::to_string(&val)
                    .map_err(|e| rhai_err(Error::YamlError(e)))
                    .map(|v| v.into())
            })
            .register_fn("yaml_encode", |val: Map| -> RhaiRes<ImmutableString> {
                serde_yaml::to_string(&val)
                    .map_err(|e| rhai_err(Error::YamlError(e)))
                    .map(|v| v.into())
            })
            .register_fn("yaml_decode", |val: ImmutableString| -> RhaiRes<Dynamic> {
                serde_yaml::from_str(val.as_ref()).map_err(|e| rhai_err(Error::YamlError(e)))
            })
            .register_fn(
                "yaml_decode_multi",
                |val: ImmutableString| -> RhaiRes<Vec<Dynamic>> {
                    let mut res = Vec::new();
                    if val.len() > 5 {
                        // non-empty string only
                        for document in serde_yaml::Deserializer::from_str(val.as_ref()) {
                            let doc =
                                Dynamic::deserialize(document).map_err(|e| rhai_err(Error::YamlError(e)))?;
                            res.push(doc);
                        }
                    }
                    Ok(res)
                },
            );
        script
            .engine
            .register_fn("file_read", |name: String| -> RhaiRes<ImmutableString> {
                std::fs::read_to_string(name)
                    .map_err(|e| rhai_err(Error::Stdio(e)))
                    .map(|v| v.into())
            })
            .register_fn("file_write", |name: String, content: String| -> RhaiRes<()> {
                std::fs::write(name, content).map_err(|e| rhai_err(Error::Stdio(e)))
            })
            .register_fn("file_copy", |source: String, dest: String| -> RhaiRes<()> {
                std::fs::copy(source, dest)
                    .map_err(|e| rhai_err(Error::Stdio(e)))
                    .map(|_| ())
            })
            .register_fn("create_dir", |name: String| -> RhaiRes<()> {
                std::fs::create_dir_all(name).map_err(|e| rhai_err(Error::Stdio(e)))
            })
            .register_fn("read_dir", |name: String| -> RhaiRes<rhai::Array> {
                let mut res = rhai::Array::new();
                for entry in std::fs::read_dir(name).map_err(|e| rhai_err(Error::Stdio(e)))? {
                    let entry = entry.map_err(|e| rhai_err(Error::Stdio(e)))?;
                    res.push(entry.path().to_str().unwrap_or_default().into());
                }
                Ok(res)
            })
            .register_fn("is_file", |name: String| -> bool { Path::new(&name).is_file() })
            .register_fn("is_dir", |name: String| -> bool { Path::new(&name).is_dir() })
            .register_fn("basename", |name: String| -> ImmutableString {
                Path::new(&name)
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or_default()
                    .into()
            })
            .register_fn("dirname", |name: String| -> ImmutableString {
                Path::new(&name)
                    .parent()
                    .unwrap()
                    .to_str()
                    .unwrap_or_default()
                    .into()
            });
        script
            .engine
            .register_type_with_name::<HandleBars>("HandleBars")
            .register_fn("new_hbs", HandleBars::new)
            .register_fn("register_template", HandleBars::rhai_register_template)
            .register_fn("register_partial_dir", HandleBars::rhai_register_partial_dir)
            .register_fn("register_helper_dir", HandleBars::rhai_register_helper_dir)
            .register_fn("render_from", HandleBars::rhai_render);
        script
            .engine
            .register_type_with_name::<RestClient>("RestClient")
            .register_fn("new_http_client", RestClient::new)
            .register_fn("headers_reset", RestClient::headers_reset_rhai)
            .register_fn("set_baseurl", RestClient::baseurl_rhai)
            .register_fn("set_server_ca", RestClient::set_server_ca)
            .register_fn("set_mtls_cert_key", RestClient::set_mtls)
            .register_fn("add_header", RestClient::add_header_rhai)
            .register_fn("add_header_json", RestClient::add_header_json)
            .register_fn("add_header_bearer", RestClient::add_header_bearer)
            .register_fn("add_header_basic", RestClient::add_header_basic)
            .register_fn("head", RestClient::rhai_head)
            .register_fn("get", RestClient::rhai_get)
            .register_fn("delete", RestClient::rhai_delete)
            .register_fn("patch", RestClient::rhai_patch)
            .register_fn("post", RestClient::rhai_post)
            .register_fn("put", RestClient::rhai_put);
        script
            .engine
            .register_type_with_name::<K8sDeploy>("K8sDeploy")
            .register_fn("get_deployment", K8sDeploy::get_deployment)
            .register_get("metadata", K8sDeploy::get_metadata)
            .register_get("spec", K8sDeploy::get_spec)
            .register_get("status", K8sDeploy::get_status)
            .register_fn("wait_available", K8sDeploy::wait_available);
        script
            .engine
            .register_type_with_name::<K8sDaemonSet>("K8sDaemonSet")
            .register_fn("get_deamonset", K8sDaemonSet::get_deamonset)
            .register_get("metadata", K8sDaemonSet::get_metadata)
            .register_get("spec", K8sDaemonSet::get_spec)
            .register_get("status", K8sDaemonSet::get_status)
            .register_fn("wait_available", K8sDaemonSet::wait_available);
        script
            .engine
            .register_type_with_name::<K8sStatefulSet>("K8sStatefulSet")
            .register_fn("get_statefulset", K8sStatefulSet::get_sts)
            .register_get("metadata", K8sStatefulSet::get_metadata)
            .register_get("spec", K8sStatefulSet::get_spec)
            .register_get("status", K8sStatefulSet::get_status)
            .register_fn("wait_available", K8sStatefulSet::wait_available);
        script
            .engine
            .register_type_with_name::<K8sJob>("K8sJob")
            .register_fn("get_job", K8sJob::get_job)
            .register_get("metadata", K8sJob::get_metadata)
            .register_get("spec", K8sJob::get_spec)
            .register_get("status", K8sJob::get_status)
            .register_fn("wait_done", K8sJob::wait_done);
        script
            .engine
            .register_type_with_name::<DynamicObject>("DynamicObject")
            .register_get("data", |obj: &mut DynamicObject| -> Dynamic {
                Dynamic::from(obj.data.clone())
            });
        script
            .engine
            .register_type_with_name::<K8sObject>("K8sObject")
            .register_get("kind", K8sObject::get_kind)
            .register_get("metadata", K8sObject::get_metadata)
            .register_fn("delete", K8sObject::rhai_delete)
            .register_fn("wait_condition", K8sObject::wait_condition)
            .register_fn("wait_deleted", K8sObject::rhai_wait_deleted)
            /*.register_fn("wait_for", |context: NativeCallContext, k8sobj: &mut K8sObject, fnp: FnPtr, timeout: i64| {
                let condition = Box::new(move |obj: &DynamicObject| -> RhaiRes<bool> {
                    fnp.call_within_context(&context, (obj.clone(),))
                });
                tracing::warn!("wait_for");
                k8sobj.wait_for(condition, timeout)
            })*/;
        script
            .engine
            .register_type_with_name::<K8sGeneric>("K8sGeneric")
            .register_fn("k8s_resource", K8sGeneric::new_global)
            .register_fn("k8s_resource", K8sGeneric::new_ns)
            .register_fn("list", K8sGeneric::rhai_list)
            .register_fn("update_k8s_crd_cache", update_cache)
            .register_fn("list_meta", K8sGeneric::rhai_list_meta)
            .register_fn("get", K8sGeneric::rhai_get)
            .register_fn("get_meta", K8sGeneric::rhai_get_meta)
            .register_fn("get_obj", K8sGeneric::rhai_get_obj)
            .register_fn("delete", K8sGeneric::rhai_delete)
            .register_fn("create", K8sGeneric::rhai_create)
            .register_fn("replace", K8sGeneric::rhai_replace)
            .register_fn("patch", K8sGeneric::rhai_patch)
            .register_fn("apply", K8sGeneric::rhai_apply)
            .register_fn("exist", K8sGeneric::rhai_exist)
            .register_get("scope", K8sGeneric::rhai_get_scope);
        script
            .engine
            .register_type_with_name::<Argon>("Argon")
            .register_fn("new_argon", Argon::new)
            .register_fn("hash", Argon::rhai_hash);
        script
            .engine
            .register_type_with_name::<Semver>("Semver")
            .register_fn("semver_from", Semver::rhai_parse)
            .register_fn("inc_major", Semver::inc_major)
            .register_fn("inc_minor", Semver::inc_minor)
            .register_fn("inc_patch", Semver::inc_patch)
            .register_fn("inc_beta", Semver::rhai_inc_beta)
            .register_fn("inc_alpha", Semver::rhai_inc_alpha)
            .register_fn("==", |a: Semver, b: Semver| a == b)
            .register_fn("!=", |a: Semver, b: Semver| a != b)
            .register_fn("<", |a: Semver, b: Semver| a < b)
            .register_fn(">", |a: Semver, b: Semver| a > b)
            .register_fn("<=", |a: Semver, b: Semver| a <= b)
            .register_fn(">=", |a: Semver, b: Semver| a >= b)
            .register_fn("to_string", Semver::to_string);
        script
            .engine
            .register_type_with_name::<DateTimeHandler>("DateTimeHandler")
            .register_fn("date_now", DateTimeHandler::now)
            .register_fn("format", DateTimeHandler::rhai_format);
        script
            .engine
            .register_type_with_name::<Registry>("Registry")
            .register_fn("new_registry", Registry::new)
            .register_fn("push_image", Registry::push_image)
            .register_fn("list_tags", Registry::rhai_list_tags)
            .register_fn("get_manifest", Registry::get_manifest);
        script
            .engine
            .register_type_with_name::<TenantInstance>("TenantInstance")
            .register_fn("get_tenant_instance", TenantInstance::rhai_get)
            .register_fn("get_tenant_name", TenantInstance::rhai_get_tenant_name)
            .register_fn(
                "get_tenant_namespaces",
                TenantInstance::rhai_get_tenant_namespaces,
            )
            .register_fn("list_tenant_instance", TenantInstance::rhai_list)
            .register_fn("options_digest", TenantInstance::get_options_digest)
            .register_fn("get_tfstate", TenantInstance::rhai_get_tfstate)
            .register_fn("get_rhaistate", TenantInstance::rhai_get_rhaistate)
            .register_fn("set_agent_started", TenantInstance::rhai_set_agent_started)
            .register_fn("set_missing_box", TenantInstance::rhai_set_missing_box)
            .register_fn("set_missing_package", TenantInstance::rhai_set_missing_package)
            .register_fn(
                "set_missing_requirement",
                TenantInstance::rhai_set_missing_requirement,
            )
            .register_fn("set_status_ready", TenantInstance::rhai_set_status_ready)
            .register_fn("set_status_vitals", TenantInstance::rhai_set_status_vitals)
            .register_fn(
                "set_status_vital_failed",
                TenantInstance::rhai_set_status_vital_failed,
            )
            .register_fn("set_status_scalables", TenantInstance::rhai_set_status_scalables)
            .register_fn(
                "set_status_scalable_failed",
                TenantInstance::rhai_set_status_scalable_failed,
            )
            .register_fn("set_status_others", TenantInstance::rhai_set_status_others)
            .register_fn(
                "set_status_other_failed",
                TenantInstance::rhai_set_status_other_failed,
            )
            .register_fn("set_tfstate", TenantInstance::rhai_set_tfstate)
            .register_fn(
                "set_status_tofu_failed",
                TenantInstance::rhai_set_status_tofu_failed,
            )
            .register_fn("set_rhaistate", TenantInstance::rhai_set_rhaistate)
            .register_fn(
                "set_status_rhai_failed",
                TenantInstance::rhai_set_status_rhai_failed,
            )
            .register_get("metadata", TenantInstance::get_metadata)
            .register_get("spec", TenantInstance::get_spec)
            .register_get("status", TenantInstance::get_status);
        script
            .engine
            .register_type_with_name::<SystemInstance>("SystemInstance")
            .register_fn("get_system_instance", SystemInstance::rhai_get)
            .register_fn("list_system_instance", SystemInstance::rhai_list)
            .register_fn("options_digest", SystemInstance::get_options_digest)
            .register_fn("get_tfstate", SystemInstance::rhai_get_tfstate)
            .register_fn("get_rhaistate", SystemInstance::rhai_get_rhaistate)
            .register_fn("set_agent_started", SystemInstance::rhai_set_agent_started)
            .register_fn("set_missing_box", SystemInstance::rhai_set_missing_box)
            .register_fn("set_missing_package", SystemInstance::rhai_set_missing_package)
            .register_fn(
                "set_missing_requirement",
                SystemInstance::rhai_set_missing_requirement,
            )
            .register_fn("set_status_ready", SystemInstance::rhai_set_status_ready)
            .register_fn("set_status_crds", SystemInstance::rhai_set_status_crds)
            .register_fn(
                "set_status_crd_failed",
                SystemInstance::rhai_set_status_crd_failed,
            )
            .register_fn("set_status_systems", SystemInstance::rhai_set_status_systems)
            .register_fn(
                "set_status_system_failed",
                SystemInstance::rhai_set_status_system_failed,
            )
            .register_fn("set_tfstate", SystemInstance::rhai_set_tfstate)
            .register_fn(
                "set_status_tofu_failed",
                SystemInstance::rhai_set_status_tofu_failed,
            )
            .register_fn("set_rhaistate", SystemInstance::rhai_set_rhaistate)
            .register_fn(
                "set_status_rhai_failed",
                SystemInstance::rhai_set_status_rhai_failed,
            )
            .register_get("metadata", SystemInstance::get_metadata)
            .register_get("spec", SystemInstance::get_spec)
            .register_get("status", SystemInstance::get_status);
        script
            .engine
            .register_type_with_name::<JukeBox>("JukeBox")
            .register_fn("get_jukebox", JukeBox::rhai_get)
            .register_fn("list_jukebox", JukeBox::rhai_list)
            .register_fn("set_status_updated", JukeBox::rhai_set_status_updated)
            .register_fn("set_status_failed", JukeBox::rhai_set_status_failed)
            .register_get("metadata", JukeBox::get_metadata)
            .register_get("spec", JukeBox::get_spec)
            .register_get("status", JukeBox::get_status);
        script
            .engine
            .register_type_with_name::<VynilPackageSource>("VynilPackage")
            .register_fn("read_package_yaml", rhai_read_package_yaml)
            .register_fn("validate_options", VynilPackageSource::validate_options)
            .register_get("metadata", VynilPackageSource::get_metadata)
            .register_get("requirements", VynilPackageSource::get_requirements)
            .register_get("options", VynilPackageSource::get_options)
            .register_get("value_script", VynilPackageSource::get_value_script)
            .register_get("images", VynilPackageSource::get_images)
            .register_get("resources", VynilPackageSource::get_resources);
        script.add_code("fn assert(cond, mess) {if (!cond){throw mess}}");
        script.add_code(
            "fn import_run(name, instance, context) {\n\
            try {\n\
                import name as imp;\n\
                return imp::run(instance, context);\n\
            } catch(e) {\n\
                if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorModuleNotFound\" {\n\
                    log_debug(`No ${name} module, skipping.`);\n\
                } else if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorFunctionNotFound\" {\n\
                    log_debug(`No ${name}::run function, skipping.`);\n\
                } else {\n\
                    throw e;\n\
                }\n\
            }\n\
        }",
        );
        script.add_code(
            "fn import_run(name, args) {\n\
            try {\n\
                import name as imp;\n\
                return imp::run(args);\n\
            } catch(e) {\n\
                if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorModuleNotFound\" {\n\
                    log_debug(`No ${name} module, skipping.`);\n\
                } else if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorFunctionNotFound\" {\n\
                    log_debug(`No ${name}::run function, skipping.`);\n\
                } else {\n\
                    throw e;\n\
                }\n\
            }\n\
        }",
        );
        script
    }

    pub fn add_code(&mut self, code: &str) {
        match self.engine.compile(code) {
            Ok(ast) => {
                match Module::eval_ast_as_new(self.ctx.clone(), &ast, &self.engine) {
                    Ok(module) => {
                        self.engine.register_global_module(module.into());
                    }
                    Err(e) => {
                        tracing::error!("Parsing {code} failed with: {e:}");
                    }
                };
            }
            Err(e) => {
                tracing::error!("Loading {code} failed with: {e:}")
            }
        };
    }

    pub fn set_dynamic(&mut self, name: &str, val: &serde_json::Value) {
        let value: Dynamic = serde_json::from_str(&serde_json::to_string(&val).unwrap()).unwrap();
        self.ctx.set_or_push(name, value);
    }

    pub fn run_file(&mut self, file: &PathBuf) -> Result<Dynamic, Error> {
        if Path::new(&file).is_file() {
            let str = file.as_os_str().to_str().unwrap();
            match self.engine.compile_file(str.into()) {
                Ok(ast) => self
                    .engine
                    .eval_ast_with_scope::<Dynamic>(&mut self.ctx, &ast)
                    .map_err(Error::RhaiError),
                Err(e) => Err(Error::RhaiError(e)),
            }
        } else {
            Err(Error::MissingScript(file.clone()))
        }
    }

    pub fn eval(&mut self, script: &str) -> Result<Dynamic, Error> {
        self.engine
            .eval_with_scope::<Dynamic>(&mut self.ctx, script)
            .map_err(RhaiError)
    }

    pub fn eval_truth(&mut self, script: &str) -> Result<bool, Error> {
        tracing::debug!("START: eval_truth({})", script);
        let r = self
            .engine
            .eval_with_scope::<bool>(&mut self.ctx, script)
            .map_err(RhaiError);
        tracing::debug!("END: eval_truth({})", script);
        r
    }

    pub fn eval_map_string(&mut self, script: &str) -> Result<String, Error> {
        tracing::debug!("START: eval_map_string({})", script);
        let m = self
            .engine
            .eval_with_scope::<Map>(&mut self.ctx, script)
            .map_err(RhaiError)?;
        tracing::debug!("END: eval_map_string({})", script);
        serde_json::to_string(&m).map_err(Error::SerializationError)
    }
}
