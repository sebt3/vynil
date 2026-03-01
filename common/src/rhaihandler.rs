use crate::{
    Error::{self, *},
    Result, RhaiRes,
    chronohandler::chrono_rhai_register,
    context,
    /*ed25519handler::Ed25519,*/
    handlebarshandler::handlebars_rhai_register,
    hasheshandlers::hashes_rhai_register,
    httphandler::http_rhai_register,
    httpmock::{HttpMockItem, httpmock_rhai_register},
    instanceservice::service_rhai_register,
    instancesystem::system_rhai_register,
    instancetenant::tenant_rhai_register,
    jukebox::jukebox_rhai_register,
    k8sgeneric::k8sgeneric_rhai_register,
    k8smock::k8smock_rhai_register,
    k8sraw::k8sraw_rhai_register,
    k8sworkload::k8sworkload_rhai_register,
    ocihandler::oci_rhai_register,
    passwordhandler::password_rhai_register,
    rhai_err,
    semverhandler::semver_rhai_register,
    shellhandler::shell_rhai_register,
    vynilpackage::package_rhai_register,
    yamlhandler::yaml_rhai_register,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
pub use rhai::{
    Array,
    Dynamic,
    Engine,
    ImmutableString,
    Map,
    Module,
    Scope,
    //    FnPtr, NativeCallContext,
    module_resolvers::{FileModuleResolver, ModuleResolversCollection},
    serde::to_dynamic,
};
use std::path::{Path, PathBuf};
use url::form_urlencoded;


pub fn base64_decode(input: String) -> Result<String> {
    String::from_utf8(STANDARD.decode(&input).unwrap()).map_err(Error::UTF8)
}
pub fn url_encode(arg: String) -> String {
    form_urlencoded::byte_serialize(arg.as_bytes()).collect::<String>()
}

fn common_rhai_register(engine: &mut Engine) {
    engine
        .register_fn("vynil_owner", || -> Dynamic {
            match context::get_owner() {
                Some(o) => serde_json::from_str(&serde_json::to_string(&o).unwrap()).unwrap(),
                None => serde_json::from_str("{}").unwrap(),
            }
        })
        .register_fn("sha256", |v: String| sha256::digest(v))
        .register_fn("log_debug", |s: ImmutableString| tracing::debug!("{s}"))
        .register_fn("log_info", |s: ImmutableString| tracing::info!("{s}"))
        .register_fn("log_warn", |s: ImmutableString| tracing::warn!("{s}"))
        .register_fn("log_error", |s: ImmutableString| tracing::error!("{s}"))
        .register_fn("url_encode", url_encode)
        .register_fn("get_env", |var: ImmutableString| -> String {
            std::env::var(var.to_string()).unwrap_or("".into())
        })
        .register_fn("to_decimal", |val: ImmutableString| -> RhaiRes<u32> {
            Ok(u32::from_str_radix(val.as_str(), 8).unwrap_or_else(|_| {
                tracing::warn!("to_decimal received a non-valid parameter: {:?}", val);
                0
            }))
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
        });
    engine
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
}

#[derive(Debug)]
pub struct Script {
    pub engine: Engine,
    pub ctx: Scope<'static>,
}
impl Script {
    #[must_use]
    pub fn new_core(resolver_path: Vec<String>) -> Script {
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
        common_rhai_register(&mut script.engine);
        chrono_rhai_register(&mut script.engine);
        hashes_rhai_register(&mut script.engine);
        password_rhai_register(&mut script.engine);
        semver_rhai_register(&mut script.engine);
        package_rhai_register(&mut script.engine);
        yaml_rhai_register(&mut script.engine);
        handlebars_rhai_register(&mut script.engine);
        oci_rhai_register(&mut script.engine);
        shell_rhai_register(&mut script.engine);
        script.add_common();
        script
    }

    pub fn new(resolver_path: Vec<String>) -> Script {
        let mut script = Self::new_core(resolver_path);
        http_rhai_register(&mut script.engine);
        service_rhai_register(&mut script.engine);
        system_rhai_register(&mut script.engine);
        tenant_rhai_register(&mut script.engine);
        jukebox_rhai_register(&mut script.engine);
        k8sgeneric_rhai_register(&mut script.engine);
        k8sraw_rhai_register(&mut script.engine);
        k8sworkload_rhai_register(&mut script.engine);
        script
    }

    pub fn new_mock(
        resolver_path: Vec<String>,
        http_mocks: Vec<HttpMockItem>,
        k8s_mocks: Vec<Dynamic>,
        created_objects: std::sync::Arc<std::sync::Mutex<Vec<Dynamic>>>,
    ) -> Script {
        let mut script = Self::new_core(resolver_path);
        httpmock_rhai_register(&mut script.engine, http_mocks);
        k8smock_rhai_register(&mut script.engine, k8s_mocks, created_objects);
        script
    }

    pub fn add_common(&mut self) {
        self.add_code("fn assert(cond, mess) {if (!cond){throw mess}}");
        self.add_code(
            "fn import_run(name, instance, context, args) {\n\
            try {\n\
                import name as imp;\n\
                return imp::run(instance, context, args);\n\
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
        self.add_code(
            "fn import_template(name, instance, context, args) {\n\
            try {\n\
                import name as imp;\n\
                return imp::template(instance, context, args);\n\
            } catch(e) {\n\
                if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorModuleNotFound\" {\n\
                    log_debug(`No ${name} module, skipping.`);\n\
                } else if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorFunctionNotFound\" {\n\
                    try {\n\
                        import name as imp;\n\
                        return imp::run(instance, context, args);\n\
                    } catch(e) {\n\
                        if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorFunctionNotFound\" {\n\
                            log_debug(`No ${name}::run function, skipping.`);\n\
                        } else {\n\
                            throw e;\n\
                        }\n\
                    }\n\
                } else {\n\
                    throw e;\n\
                }\n\
            }\n\
        }",
        );
        self.add_code(
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
        self.add_code(
            "fn import_template(name, instance, context) {\n\
            try {\n\
                import name as imp;\n\
                return imp::template(instance, context);\n\
            } catch(e) {\n\
                if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorModuleNotFound\" {\n\
                    log_debug(`No ${name} module, skipping.`);\n\
                } else if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorFunctionNotFound\" {\n\
                    try {\n\
                        import name as imp;\n\
                        return imp::run(instance, context);\n\
                    } catch(e) {\n\
                        if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorFunctionNotFound\" {\n\
                            log_debug(`No ${name}::run function, skipping.`);\n\
                        } else {\n\
                            throw e;\n\
                        }\n\
                    }\n\
                } else {\n\
                    throw e;\n\
                }\n\
            }\n\
        }",
        );
        self.add_code(
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
        self.add_code(
            "fn import_template(name, args) {\n\
            try {\n\
                import name as imp;\n\
                return imp::template(args);\n\
            } catch(e) {\n\
                if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorModuleNotFound\" {\n\
                    log_debug(`No ${name} module, skipping.`);\n\
                } else if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorFunctionNotFound\" {\n\
                    try {\n\
                        import name as imp;\n\
                        return imp::run(args);\n\
                    } catch(e) {\n\
                        if type_of(e) == \"map\" && \"error\" in e && e.error == \"ErrorFunctionNotFound\" {\n\
                            log_debug(`No ${name}::run function, skipping.`);\n\
                        } else {\n\
                            throw e;\n\
                        }\n\
                    }\n\
                } else {\n\
                    throw e;\n\
                }\n\
            }\n\
        }",
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_script() -> Script {
        Script::new(vec![])
    }

    // ── yaml_decode / yaml_encode ─────────────────────────────────────────────

    #[test]
    fn test_yaml_decode_string_value() {
        let mut s = make_script();
        let result = s.eval(r#"yaml_decode("key: hello")["key"]"#).unwrap();
        assert_eq!(result.to_string(), "hello");
    }

    #[test]
    fn test_yaml_decode_integer_value() {
        let mut s = make_script();
        let result = s.eval(r#"yaml_decode("count: 42")["count"]"#).unwrap();
        assert_eq!(result.cast::<i64>(), 42);
    }

    #[test]
    fn test_yaml_decode_boolean_value() {
        let mut s = make_script();
        let result = s.eval(r#"yaml_decode("enabled: true")["enabled"]"#).unwrap();
        assert_eq!(result.cast::<bool>(), true);
    }

    #[test]
    fn test_yaml_decode_nested_access() {
        let mut s = make_script();
        let result = s.eval(r#"yaml_decode("a:\n  b: nested")["a"]["b"]"#).unwrap();
        assert_eq!(result.to_string(), "nested");
    }

    #[test]
    fn test_yaml_decode_array_access() {
        let mut s = make_script();
        let result = s
            .eval(r#"yaml_decode("items:\n  - first\n  - second")["items"][1]"#)
            .unwrap();
        assert_eq!(result.to_string(), "second");
    }

    #[test]
    fn test_yaml_encode_produces_yaml() {
        let mut s = make_script();
        let result = s.eval(r#"yaml_encode(#{"key": "value"})"#).unwrap();
        let yaml_str = result.to_string();
        assert!(yaml_str.contains("key:"));
        assert!(yaml_str.contains("value"));
    }

    #[test]
    fn test_yaml_encode_decode_roundtrip() {
        // Encode a map, decode it back, access a key
        let mut s = make_script();
        let result = s
            .eval(
                r#"
            let m = #{"name": "test", "count": 3};
            let encoded = yaml_encode(m);
            let decoded = yaml_decode(encoded);
            decoded["name"]
        "#,
            )
            .unwrap();
        assert_eq!(result.to_string(), "test");
    }

    // ── yaml_decode_multi ─────────────────────────────────────────────────────

    #[test]
    fn test_yaml_decode_multi_single_document() {
        let mut s = make_script();
        let result = s.eval(r#"yaml_decode_multi("key: val\n").len()"#).unwrap();
        assert_eq!(result.cast::<i64>(), 1);
    }

    #[test]
    fn test_yaml_decode_multi_two_documents() {
        let mut s = make_script();
        let result = s
            .eval(r#"yaml_decode_multi("key: a\n---\nkey: b\n").len()"#)
            .unwrap();
        assert_eq!(result.cast::<i64>(), 2);
    }

    #[test]
    fn test_yaml_decode_multi_document_values() {
        let mut s = make_script();
        let result = s
            .eval(
                r#"
            let docs = yaml_decode_multi("key: first\n---\nkey: second\n");
            docs[1]["key"]
        "#,
            )
            .unwrap();
        assert_eq!(result.to_string(), "second");
    }

    #[test]
    fn test_yaml_decode_multi_short_string_returns_empty() {
        // String shorter than 5 chars → empty result (guard in the code)
        let mut s = make_script();
        let result = s.eval(r#"yaml_decode_multi("ab").len()"#).unwrap();
        assert_eq!(result.cast::<i64>(), 0);
    }

    // ── json_encode / json_decode ─────────────────────────────────────────────

    #[test]
    fn test_json_encode_decode_roundtrip() {
        let mut s = make_script();
        let result = s
            .eval(
                r#"
            let encoded = json_encode(#{"a": "hello", "b": 42});
            let decoded = json_decode(encoded);
            decoded["a"]
        "#,
            )
            .unwrap();
        assert_eq!(result.to_string(), "hello");
    }

    #[test]
    fn test_json_decode_invalid_returns_error() {
        let mut s = make_script();
        assert!(s.eval(r#"json_decode("not json")"#).is_err());
    }

    // ── base64_encode / base64_decode ─────────────────────────────────────────

    #[test]
    fn test_base64_encode_decode_roundtrip() {
        let mut s = make_script();
        let result = s
            .eval(
                r#"
            let encoded = base64_encode("hello world");
            base64_decode(encoded)
        "#,
            )
            .unwrap();
        assert_eq!(result.to_string(), "hello world");
    }

    #[test]
    fn test_base64_encode_known_value() {
        let mut s = make_script();
        let result = s.eval(r#"base64_encode("hello")"#).unwrap();
        assert_eq!(result.to_string(), "aGVsbG8=");
    }

    // ── Semver from Rhai ──────────────────────────────────────────────────────

    #[test]
    fn test_semver_parse_and_to_string() {
        let mut s = make_script();
        let result = s.eval(r#"to_string(semver_from("1.2.3"))"#).unwrap();
        assert_eq!(result.to_string(), "1.2.3");
    }

    #[test]
    fn test_semver_comparison_operators() {
        let mut s = make_script();
        assert_eq!(
            s.eval(r#"semver_from("1.0.0") < semver_from("2.0.0")"#)
                .unwrap()
                .cast::<bool>(),
            true
        );
        assert_eq!(
            s.eval(r#"semver_from("2.0.0") > semver_from("1.0.0")"#)
                .unwrap()
                .cast::<bool>(),
            true
        );
        assert_eq!(
            s.eval(r#"semver_from("1.0.0") == semver_from("1.0.0")"#)
                .unwrap()
                .cast::<bool>(),
            true
        );
    }

    #[test]
    fn test_semver_inc_minor() {
        let mut s = make_script();
        let result = s
            .eval(
                r#"
            let v = semver_from("1.2.3");
            inc_minor(v);
            to_string(v)
        "#,
            )
            .unwrap();
        assert_eq!(result.to_string(), "1.3.0");
    }

    // ── Utility functions ─────────────────────────────────────────────────────

    #[test]
    fn test_sha256_known_hash() {
        let mut s = make_script();
        let result = s.eval(r#"sha256("hello")"#).unwrap();
        assert_eq!(
            result.to_string(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_to_decimal_octal() {
        let mut s = make_script();
        let result = s.eval(r#"to_decimal("755")"#).unwrap();
        assert_eq!(result.cast::<u32>(), 493);
    }

    #[test]
    fn test_url_encode() {
        let mut s = make_script();
        let result = s.eval(r#"url_encode("hello world")"#).unwrap();
        assert_eq!(result.to_string(), "hello+world");
    }

    // ── Régression : préservation de l'ordre des clés YAML ───────────────────
    //
    // Scénario : lire un YAML avec des clés dans un ordre non-alphabétique via
    // yaml_decode_ordered, modifier une seule valeur, re-sérialiser via
    // yaml_encode_ordered → une seule ligne doit changer.
    #[test]
    fn test_yaml_decode_modify_single_key_preserves_order() {
        let mut s = make_script();

        // Clés dans l'ordre z → a → m (intentionnellement non-alphabétique)
        let original = "z: first\na: second\nm: third\n";
        s.ctx
            .set_or_push("input_yaml", Dynamic::from(original.to_string()));

        let result = s
            .eval(
                r#"
            let doc = yaml_decode_ordered(input_yaml);
            doc["a"] = "MODIFIED";
            yaml_encode_ordered(doc)
        "#,
            )
            .unwrap();

        let encoded = result.to_string();

        // Comparer ligne par ligne (on ignore les lignes vides trailing)
        let orig_lines: Vec<&str> = original.lines().filter(|l| !l.is_empty()).collect();
        let enc_lines: Vec<&str> = encoded.lines().filter(|l| !l.is_empty()).collect();

        assert_eq!(
            orig_lines.len(),
            enc_lines.len(),
            "Le nombre de lignes a changé — des clés ont été perdues ou ajoutées.\n\
             Original:\n{}\nRésultat:\n{}",
            original,
            encoded
        );

        let changed: Vec<usize> = orig_lines
            .iter()
            .zip(enc_lines.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect();

        assert_eq!(
            changed.len(),
            1,
            "Attendu exactement 1 ligne modifiée (la clé 'a'), mais {} lignes diffèrent.\n\
             Cela indique que l'ordre des clés n'a PAS été préservé (bug serde_yaml/BTreeMap).\n\
             Original:\n{}\nRésultat:\n{}",
            changed.len(),
            original,
            encoded
        );

        assert!(
            enc_lines[changed[0]].contains("MODIFIED"),
            "La ligne modifiée ne contient pas 'MODIFIED' : {}",
            enc_lines[changed[0]]
        );
    }
}
