use super::{
    result::TestResultCollector,
    vyniltest::{VynilTest, VynilTestSetRef, appslug},
    vyniltestset::{VynilAssert, VynilAssertResult, VynilAssertSelector, VynilTestSet, VynilTestSetMocks},
};
use common::{
    handlebarshandler::HandleBars,
    httpmock::HttpMockItem,
    rhaihandler::Script,
    vynilpackage::{VynilPackageSource, VynilPackageType, read_package_yaml},
};
use rhai::{Dynamic, Map};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const API_VERSION: &str = "vinyl.solidite.fr/v1beta1";

// ── Templating helpers (private) ────────────────────────────────────────────

/// Recursively templates all string keys and string values inside a serde_json::Value.
fn template_json(
    hbs: &mut HandleBars,
    ctx: &serde_json::Value,
    val: serde_json::Value,
) -> common::Result<serde_json::Value> {
    match val {
        serde_json::Value::String(s) => Ok(serde_json::Value::String(hbs.render(&s, ctx)?)),
        serde_json::Value::Array(arr) => Ok(serde_json::Value::Array(
            arr.into_iter()
                .map(|v| template_json(hbs, ctx, v))
                .collect::<common::Result<_>>()?,
        )),
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(hbs.render(&k, ctx)?, template_json(hbs, ctx, v)?);
            }
            Ok(serde_json::Value::Object(out))
        }
        other => Ok(other),
    }
}

/// Templates all strings inside a rhai::Dynamic (via JSON round-trip).
fn template_dynamic(hbs: &mut HandleBars, ctx: &serde_json::Value, d: &Dynamic) -> common::Result<Dynamic> {
    let json: serde_json::Value = serde_json::from_str(
        &serde_json::to_string(d).map_err(common::Error::SerializationError)?,
    )
    .map_err(common::Error::SerializationError)?;
    let templated = template_json(hbs, ctx, json)?;
    serde_json::from_str(
        &serde_json::to_string(&templated).map_err(common::Error::SerializationError)?,
    )
    .map_err(common::Error::SerializationError)
}

/// Templates all string keys and values inside a rhai::Map.
fn template_rhai_map(hbs: &mut HandleBars, ctx: &serde_json::Value, map: &Map) -> common::Result<Map> {
    let d = template_dynamic(hbs, ctx, &Dynamic::from_map(map.clone()))?;
    d.try_cast::<Map>()
        .ok_or_else(|| common::Error::Other("expected map after templating".into()))
}

fn template_http_mock(
    hbs: &mut HandleBars,
    ctx: &serde_json::Value,
    item: &HttpMockItem,
) -> common::Result<HttpMockItem> {
    Ok(HttpMockItem {
        path: hbs.render(&item.path, ctx)?,
        method: item.method.clone(),
        return_obj: template_rhai_map(hbs, ctx, &item.return_obj)?,
    })
}

fn template_assert(
    hbs: &mut HandleBars,
    ctx: &serde_json::Value,
    a: &VynilAssert,
) -> common::Result<VynilAssert> {
    Ok(VynilAssert {
        name: a.name.clone(),
        description: a.description.clone(),
        selector: VynilAssertSelector {
            kind: a.selector.kind.as_ref().map(|s| hbs.render(s, ctx)).transpose()?,
            name: a.selector.name.as_ref().map(|s| hbs.render(s, ctx)).transpose()?,
            namespace: a
                .selector
                .namespace
                .as_ref()
                .map(|s| hbs.render(s, ctx))
                .transpose()?,
        },
        matcher: a.matcher.clone(),
        value: if a.value.is_some() {Some(template_json(hbs, ctx, a.value.clone().unwrap())?)} else {None},
    })
}

/// Builds the `var` context for a testSet reference:
/// ref variable values take priority, then testSet defaults.
fn build_var_context(ts: &VynilTestSet, ts_ref: &VynilTestSetRef) -> serde_json::Value {
    let mut var = serde_json::Map::new();
    if let Some(variables) = &ts.variables {
        for (name, def) in variables {
            let value = ts_ref
                .variables
                .as_ref()
                .and_then(|v| v.get(name))
                .cloned()
                .or_else(|| def.default.clone())
                .unwrap_or(serde_json::Value::Null);
            var.insert(name.clone(), value);
        }
    }
    // Include ref variables not declared in the testSet definition
    if let Some(ref_vars) = &ts_ref.variables {
        for (name, value) in ref_vars {
            if !var.contains_key(name) {
                var.insert(name.clone(), value.clone());
            }
        }
    }
    serde_json::Value::Object(var)
}

/// Templates mocks/asserts from a source, appending to the collectors.
fn collect_templated(
    hbs: &mut HandleBars,
    ctx: &serde_json::Value,
    mocks: Option<&VynilTestSetMocks>,
    asserts: Option<&Vec<VynilAssert>>,
    k8s_out: &mut Vec<Dynamic>,
    http_out: &mut Vec<HttpMockItem>,
    asserts_out: &mut Vec<VynilAssert>,
) -> common::Result<()> {
    let mut context = ctx.clone();
    context.as_object_mut().unwrap().insert("context".into(), ctx.clone());

    if let Some(m) = mocks {
        if let Some(k8s) = &m.kubernetes {
            for d in k8s {
                k8s_out.push(template_dynamic(hbs, &context, d)?);
            }
        }
        if let Some(http) = &m.http {
            for item in http {
                http_out.push(template_http_mock(hbs, &context, item)?);
            }
        }
    }
    if let Some(list) = asserts {
        for a in list {
            asserts_out.push(template_assert(hbs, &context, a)?);
        }
    }
    Ok(())
}

// ── TestHandler ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct TestHandler {
    package_dir: PathBuf,
    script_dir: PathBuf,
    config_dir: PathBuf,
    template_dir: PathBuf,
    package: VynilPackageSource,
    pub results: TestResultCollector,
    tests: BTreeMap<String, VynilTest>,
    test_sets: BTreeMap<String, VynilTestSet>,
}

impl TestHandler {
    pub fn new(
        package_dir: PathBuf,
        script_dir: PathBuf,
        config_dir: PathBuf,
        template_dir: PathBuf,
        testset_dirs: Option<Vec<PathBuf>>,
    ) -> common::Result<Self> {
        let package = read_package_yaml(&package_dir.join("package.yaml"))?;
        let mut handler = Self {
            package_dir,
            script_dir,
            config_dir,
            template_dir,
            package,
            results: TestResultCollector::new(),
            tests: BTreeMap::new(),
            test_sets: BTreeMap::new(),
        };
        if let Some(ts_dirs) = testset_dirs {
            for dir in &ts_dirs {
                handler.load_testsets_from_dir(dir)?;
            }
        }
        handler.load_tests_from_dir(&handler.package_dir.join("tests"))?;
        Ok(handler)
    }

    /// Scans a directory for .yml/.yaml files (potentially multi-document)
    /// and loads all matching Test and TestSet objects.
    /// Objects with a different apiVersion or kind are silently ignored.
    /// Errors if a Test/TestSet name collides or if parsing fails.
    fn load_tests_from_dir(&mut self, dir: &Path) -> common::Result<()> {
        for (api_kind, json, file) in Self::scan_yaml_dir(dir)? {
            match api_kind.as_str() {
                "Test" => {
                    let test: VynilTest = serde_json::from_value(json)
                        .map_err(|e| common::Error::YamlError(format!("{}: {e}", file)))?;
                    let name = test.metadata.name.clone();
                    if self.tests.contains_key(&name) {
                        return Err(common::Error::YamlError(format!(
                            "{file}: duplicate Test '{name}'"
                        )));
                    }
                    self.tests.insert(name, test);
                }
                "TestSet" => {
                    let ts: VynilTestSet = serde_json::from_value(json)
                        .map_err(|e| common::Error::YamlError(format!("{}: {e}", file)))?;
                    let name = ts.metadata.name.clone();
                    if self.test_sets.contains_key(&name) {
                        return Err(common::Error::YamlError(format!(
                            "{file}: duplicate TestSet '{name}'"
                        )));
                    }
                    self.test_sets.insert(name, ts);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Scans a directory for .yml/.yaml files and loads only TestSet objects.
    /// Errors if a TestSet name collides or if parsing fails.
    fn load_testsets_from_dir(&mut self, dir: &Path) -> common::Result<()> {
        for (api_kind, json, file) in Self::scan_yaml_dir(dir)? {
            if api_kind != "TestSet" {
                continue;
            }
            let ts: VynilTestSet = serde_json::from_value(json)
                .map_err(|e| common::Error::YamlError(format!("{}: {e}", file)))?;
            let name = ts.metadata.name.clone();
            if self.test_sets.contains_key(&name) {
                return Err(common::Error::YamlError(format!(
                    "{file}: duplicate TestSet '{name}'"
                )));
            }
            self.test_sets.insert(name, ts);
        }
        Ok(())
    }

    pub fn get_test(&self, name: &str) -> Option<&VynilTest> {
        self.tests.get(name)
    }

    pub fn get_test_set(&self, name: &str) -> Option<&VynilTestSet> {
        self.test_sets.get(name)
    }

    pub fn list_tests(&self) -> Vec<String> {
        self.tests.keys().cloned().collect()
    }

    /// Returns a fully resolved VynilTest: handlebars templates in mocks and
    /// asserts are evaluated, and all referenced testSets are merged in.
    pub fn get_templated_test(&self, name: &str) -> common::Result<VynilTest> {
        let test = self
            .tests
            .get(name)
            .ok_or_else(|| common::Error::Other(format!("Test '{name}' not found")))?;
        let package = &self.package;

        let mut hbs = HandleBars::new();

        // Build base context: package + instance (with appslug)
        let slug = appslug(&package.metadata.name, &test.instance.name);
        let mut inst = serde_json::to_value(&test.instance)?
            .as_object()
            .cloned()
            .unwrap_or_default();
        inst.insert("appslug".to_string(), serde_json::Value::String(slug));
        let base_ctx = serde_json::json!({
            "package": serde_json::to_value(package)?,
            "instance": serde_json::Value::Object(inst),
        });

        let mut k8s_mocks: Vec<Dynamic> = Vec::new();
        let mut http_mocks: Vec<HttpMockItem> = Vec::new();
        let mut asserts: Vec<VynilAssert> = Vec::new();

        // Template test's own mocks/asserts with base context
        collect_templated(
            &mut hbs,
            &base_ctx,
            test.mocks.as_ref(),
            test.asserts.as_ref(),
            &mut k8s_mocks,
            &mut http_mocks,
            &mut asserts,
        )?;

        // Merge each referenced testSet
        if let Some(refs) = &test.testSets {
            for ts_ref in refs {
                let ts = self
                    .test_sets
                    .get(&ts_ref.testSet)
                    .ok_or_else(|| common::Error::Other(format!("TestSet '{}' not found", ts_ref.testSet)))?;
                let mut ctx = base_ctx.clone();
                ctx.as_object_mut()
                    .unwrap()
                    .insert("var".to_string(), build_var_context(ts, ts_ref));

                collect_templated(
                    &mut hbs,
                    &ctx,
                    ts.mocks.as_ref(),
                    ts.asserts.as_ref(),
                    &mut k8s_mocks,
                    &mut http_mocks,
                    &mut asserts,
                )?;
            }
        }

        // Build a virtual instance mock for the instance being tested,
        // so Rhai scripts can find it via get_{service,system,tenant}_instance().
        let instance_kind = match &package.metadata.usage {
            VynilPackageType::Service => "ServiceInstance",
            VynilPackageType::System => "SystemInstance",
            VynilPackageType::Tenant => "TenantInstance",
        };
        let already_mocked = k8s_mocks.iter().any(|m| {
            let Ok(map) = m.as_map_ref() else { return false };
            let kind_ok = map
                .get("kind")
                .and_then(|v| v.clone().into_string().ok())
                .as_deref()
                == Some(instance_kind);
            if !kind_ok {
                return false;
            }
            let Some(meta) = map.get("metadata") else {
                return false;
            };
            let Ok(meta_map) = meta.as_map_ref() else {
                return false;
            };
            let name_ok = meta_map
                .get("name")
                .and_then(|v| v.clone().into_string().ok())
                .as_deref()
                == Some(&test.instance.name);
            let ns_ok = meta_map
                .get("namespace")
                .and_then(|v| v.clone().into_string().ok())
                .as_deref()
                == Some(&test.instance.namespace);
            name_ok && ns_ok
        });
        if !already_mocked {
            let mut spec = serde_json::Map::new();
            spec.insert(
                "category".to_string(),
                serde_json::Value::String(package.metadata.category.clone()),
            );
            spec.insert(
                "package".to_string(),
                serde_json::Value::String(package.metadata.name.clone()),
            );
            if let Some(opts) = &test.instance.options {
                spec.insert(
                    "options".to_string(),
                    serde_json::Value::Object(opts.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
                );
            }
            let instance_obj = serde_json::json!({
                "apiVersion": "vynil.solidite.fr/v1",
                "kind": instance_kind,
                "metadata": {
                    "name": test.instance.name,
                    "namespace": test.instance.namespace,
                },
                "spec": serde_json::Value::Object(spec),
                "status": {},
            });
            let d: Dynamic = serde_json::from_str(&serde_json::to_string(&instance_obj).unwrap()).unwrap();
            k8s_mocks.push(d);
        }

        Ok(VynilTest {
            apiVersion: test.apiVersion.clone(),
            kind: test.kind.clone(),
            metadata: test.metadata.clone(),
            instance: test.instance.clone(),
            testSets: None,
            mocks: if k8s_mocks.is_empty() && http_mocks.is_empty() {
                None
            } else {
                Some(VynilTestSetMocks {
                    kubernetes: if k8s_mocks.is_empty() {
                        None
                    } else {
                        Some(k8s_mocks)
                    },
                    http: if http_mocks.is_empty() {
                        None
                    } else {
                        Some(http_mocks)
                    },
                })
            },
            asserts: if asserts.is_empty() { None } else { Some(asserts) },
        })
    }

    /// Validates that every TestSet referenced in a Test's testSets exists.
    pub fn validate_refs(&self) -> common::Result<()> {
        for (test_name, test) in &self.tests {
            if let Some(refs) = &test.testSets {
                for ts_ref in refs {
                    if !self.test_sets.contains_key(&ts_ref.testSet) {
                        return Err(common::Error::Other(format!(
                            "Test '{test_name}' references unknown TestSet '{}'",
                            ts_ref.testSet
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn run_all_tests(&mut self) {
        let names = self.list_tests();
        for name in &names {
            // Shared vec where the mock k8s layer records created objects
            let created_objects: Arc<Mutex<Vec<Dynamic>>> = Default::default();
            self.run_test(name, created_objects);
        }
    }

    pub fn run_test(&mut self, name: &str, created_objects: Arc<Mutex<Vec<Dynamic>>>) {
        let start = std::time::Instant::now();
        let result = self.run_test_inner(name, created_objects);
        let duration = start.elapsed();

        match result {
            Ok(asserts) => {
                self.results.add(name.to_string(), asserts, duration);
            }
            Err(e) => {
                let fail = VynilAssertResult {
                    name: "execution".to_string(),
                    description: Some("Test execution".to_string()),
                    passed: false,
                    message: format!("{e}"),
                };
                self.results.add(name.to_string(), vec![fail], duration);
            }
        }
    }

    fn run_test_inner(&self, name: &str, created_objects: Arc<Mutex<Vec<Dynamic>>>) -> common::Result<Vec<VynilAssertResult>> {
        // Resolve test: merge testSets, template mocks/asserts
        let test = self.get_templated_test(name)?;

        // Extract mocks and asserts from resolved test
        let k8s_mocks = test
            .mocks
            .as_ref()
            .and_then(|m| m.kubernetes.clone())
            .unwrap_or_default();
        let http_mocks = test
            .mocks
            .as_ref()
            .and_then(|m| m.http.clone())
            .unwrap_or_default();

        // Build resolver path
        let type_dir = match self.package.metadata.usage {
            VynilPackageType::Tenant => "tenant",
            VynilPackageType::System => "system",
            VynilPackageType::Service => "service",
        };
        let resolver_path = vec![
            format!("{}/scripts", self.package_dir.display()),
            self.config_dir.display().to_string(),
            format!("{}/{type_dir}", self.script_dir.display()),
            format!("{}/lib", self.script_dir.display()),
        ];

        // Create mock rhai interpreter
        let mut rhai = Script::new_mock(resolver_path, http_mocks, k8s_mocks, created_objects.clone());

        // Execute value_script if present, otherwise empty map
        let mut asserts: Vec<VynilAssertResult> = Vec::new();
        let controller_values = if let Some(ref vs) = self.package.value_script {
            match rhai.eval(vs) {
                Ok(val) => {
                    asserts.push(VynilAssertResult {
                        name: "value_script".to_string(),
                        description: Some("Value script execution".to_string()),
                        passed: true,
                        message: "value script executed successfully".to_string(),
                    });
                    serde_json::to_string(&rhai::serde::to_dynamic(&val).unwrap_or_default())
                        .unwrap_or_else(|_| "{}".to_string())
                }
                Err(e) => {
                    asserts.push(VynilAssertResult {
                        name: "value_script".to_string(),
                        description: Some("Value script execution".to_string()),
                        passed: false,
                        message: format!("{e}"),
                    });
                    return Ok(asserts);
                }
            }
        } else {
            "{}".to_string()
        };

        // Build args for the rhai script
        let args = serde_json::json!({
            "namespace": test.instance.namespace,
            "instance": test.instance.name,
            "vynil_namespace": "vynil-system",
            "package_dir": self.package_dir.display().to_string(),
            "script_dir": self.script_dir.display().to_string(),
            "template_dir": self.template_dir.display().to_string(),
            "agent_image": common::DEFAULT_AGENT_IMAGE,
            "tag": "0.1.0",
            "config_dir": self.config_dir.display().to_string(),
            "controller_values": controller_values,
        });
        rhai.set_dynamic("args", &args);

        // Build instance object for rhai
        let mut instance_json = serde_json::json!({
            "metadata": {
                "name": test.instance.name,
                "namespace": test.instance.namespace,
            },
            "spec": {
                "category": self.package.metadata.category,
                "package": self.package.metadata.name,
            },
            "status": {},
        });
        if let Some(opts) = &test.instance.options {
            instance_json["spec"]["options"] =
                serde_json::Value::Object(opts.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        }
        //rhai.set_dynamic("instance", &instance_json);
        let fun_name = match self.package.metadata.usage {
            VynilPackageType::Tenant => "get_tenant_instance",
            VynilPackageType::System => "get_system_instance",
            VynilPackageType::Service => "get_service_instance",
        };

        // Run the install script
        let _ = rhai.eval(
            format!("import(\"context\") as ctx;\n\
            let instance = {}(args.namespace, args.instance);\n\
            let context = ctx::run(instance, args);\n\
            import(\"install\") as install;\n\
            install::run(instance, context);", fun_name).as_str(),
        )?;

        // Validate asserts against created objects
        let objects = created_objects.lock().unwrap();
        asserts.extend(test.run_asserts(&objects));

        Ok(asserts)
    }

    /// Reads all .yml/.yaml files in `dir`, parses multi-doc YAML,
    /// and yields (kind, serde_json::Value, filename) for each document
    /// matching our apiVersion.
    fn scan_yaml_dir(dir: &Path) -> common::Result<Vec<(String, serde_json::Value, String)>> {
        let mut results = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            match path.extension().and_then(|e| e.to_str()) {
                Some("yml" | "yaml") => {}
                _ => continue,
            }
            let display = path.display().to_string();
            let content = std::fs::read_to_string(&path)?;
            for de in serde_yaml::Deserializer::from_str(&content) {
                let json: serde_json::Value = serde::Deserialize::deserialize(de)
                    .map_err(|e| common::Error::YamlError(format!("{display}: {e}")))?;
                let api_version = json.get("apiVersion").and_then(|v| v.as_str()).map(String::from);
                let kind = json.get("kind").and_then(|v| v.as_str()).map(String::from);
                if api_version.as_deref() != Some(API_VERSION) {
                    continue;
                }
                if let Some(kind) = kind {
                    results.push((kind, json, display.clone()));
                }
            }
        }
        Ok(results)
    }
}
