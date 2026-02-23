use std::collections::BTreeMap;
use std::path::Path;
use serde::{Deserialize, Serialize};
use rhai::{Dynamic, Map};
use rust_yaml::Value;
use crate::handlebarshandler::HandleBars;
use crate::httpmock::HttpMockItem;
use crate::vynilpackage::VynilPackageSource;
use crate::vyniltestset::{VynilAssert, VynilAssertSelector, VynilTestSet, VynilTestSetMocks};
use crate::yamlhandler::{
    YamlDoc, dynamic_to_value, serde_json_to_yaml_value, value_to_rhai_dynamic,
    yaml_value_to_serde_json,
};

const API_VERSION: &str = "vinyl.solidite.fr/v1beta1";

pub fn appslug(pkg: &str, inst: &str) -> String {
    if pkg == inst {
        inst.to_string()
    } else {
        let slug = format!("{inst}-{pkg}");
        if slug.chars().count() > 28 {
            slug.chars().take(28).collect()
        } else {
            slug
        }
    }
}

/// Vynil Test
#[allow(non_snake_case)]
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilTest {
    pub apiVersion: String,
    pub kind: String,
    /// Metadata for a test
    pub metadata: VynilTestMeta,
    /// Target instance to test against
    pub instance: VynilTestInstance,
    /// TestSet references with variable overrides
    pub testSets: Option<Vec<VynilTestSetRef>>,
    /// Additional mocks at test level
    pub mocks: Option<VynilTestSetMocks>,
    /// Assert definitions
    pub asserts: Option<Vec<VynilAssert>>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilTestMeta {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilTestInstance {
    pub name: String,
    pub namespace: String,
    pub options: Option<BTreeMap<String, serde_json::Value>>,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VynilTestSetRef {
    pub testSet: String,
    pub variables: Option<BTreeMap<String, serde_json::Value>>,
}

// ── Templating helpers (private) ────────────────────────────────────────────

/// Recursively templates all string keys and string values inside a serde_json::Value.
fn template_json(
    hbs: &mut HandleBars,
    ctx: &serde_json::Value,
    val: serde_json::Value,
) -> crate::Result<serde_json::Value> {
    match val {
        serde_json::Value::String(s) => Ok(serde_json::Value::String(hbs.render(&s, ctx)?)),
        serde_json::Value::Array(arr) => Ok(serde_json::Value::Array(
            arr.into_iter()
                .map(|v| template_json(hbs, ctx, v))
                .collect::<crate::Result<_>>()?,
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
fn template_dynamic(
    hbs: &mut HandleBars,
    ctx: &serde_json::Value,
    d: &Dynamic,
) -> crate::Result<Dynamic> {
    let json = yaml_value_to_serde_json(dynamic_to_value(d.clone()));
    let templated = template_json(hbs, ctx, json)?;
    Ok(value_to_rhai_dynamic(serde_json_to_yaml_value(templated)))
}

/// Templates all string keys and values inside a rhai::Map.
fn template_rhai_map(
    hbs: &mut HandleBars,
    ctx: &serde_json::Value,
    map: &Map,
) -> crate::Result<Map> {
    let d = template_dynamic(hbs, ctx, &Dynamic::from_map(map.clone()))?;
    d.try_cast::<Map>()
        .ok_or_else(|| crate::Error::Other("expected map after templating".into()))
}

fn template_http_mock(
    hbs: &mut HandleBars,
    ctx: &serde_json::Value,
    item: &HttpMockItem,
) -> crate::Result<HttpMockItem> {
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
) -> crate::Result<VynilAssert> {
    Ok(VynilAssert {
        selector: VynilAssertSelector {
            kind: a.selector.kind.as_ref().map(|s| hbs.render(s, ctx)).transpose()?,
            name: a.selector.name.as_ref().map(|s| hbs.render(s, ctx)).transpose()?,
            namespace: a.selector.namespace.as_ref().map(|s| hbs.render(s, ctx)).transpose()?,
        },
        matcher: a.matcher.clone(),
        value: template_json(hbs, ctx, a.value.clone())?,
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
) -> crate::Result<()> {
    if let Some(m) = mocks {
        if let Some(k8s) = &m.kubernetes {
            for d in k8s {
                k8s_out.push(template_dynamic(hbs, ctx, d)?);
            }
        }
        if let Some(http) = &m.http {
            for item in http {
                http_out.push(template_http_mock(hbs, ctx, item)?);
            }
        }
    }
    if let Some(list) = asserts {
        for a in list {
            asserts_out.push(template_assert(hbs, ctx, a)?);
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub struct TestHandler {
    pub tests: BTreeMap<String, VynilTest>,
    pub test_sets: BTreeMap<String, VynilTestSet>,
}

impl TestHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Scans a directory for .yml/.yaml files (potentially multi-document)
    /// and loads all matching Test and TestSet objects.
    /// Objects with a different apiVersion or kind are silently ignored.
    /// Errors if a Test/TestSet name collides or if parsing fails.
    pub fn load_tests_from_dir(&mut self, dir: &Path) -> crate::Result<()> {
        for (api_kind, json, file) in Self::scan_yaml_dir(dir)? {
            match api_kind.as_str() {
                "Test" => {
                    let test: VynilTest = serde_json::from_value(json)
                        .map_err(|e| crate::Error::YamlError(format!("{}: {e}", file)))?;
                    let name = test.metadata.name.clone();
                    if self.tests.contains_key(&name) {
                        return Err(crate::Error::YamlError(
                            format!("{file}: duplicate Test '{name}'")));
                    }
                    self.tests.insert(name, test);
                }
                "TestSet" => {
                    let ts: VynilTestSet = serde_json::from_value(json)
                        .map_err(|e| crate::Error::YamlError(format!("{}: {e}", file)))?;
                    let name = ts.metadata.name.clone();
                    if self.test_sets.contains_key(&name) {
                        return Err(crate::Error::YamlError(
                            format!("{file}: duplicate TestSet '{name}'")));
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
    pub fn load_testsets_from_dir(&mut self, dir: &Path) -> crate::Result<()> {
        for (api_kind, json, file) in Self::scan_yaml_dir(dir)? {
            if api_kind != "TestSet" {
                continue;
            }
            let ts: VynilTestSet = serde_json::from_value(json)
                .map_err(|e| crate::Error::YamlError(format!("{}: {e}", file)))?;
            let name = ts.metadata.name.clone();
            if self.test_sets.contains_key(&name) {
                return Err(crate::Error::YamlError(
                    format!("{file}: duplicate TestSet '{name}'")));
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
    pub fn get_templated_test(
        &self,
        name: &str,
        package: &VynilPackageSource,
    ) -> crate::Result<VynilTest> {
        let test = self.tests.get(name).ok_or_else(|| {
            crate::Error::Other(format!("Test '{name}' not found"))
        })?;

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
                let ts = self.test_sets.get(&ts_ref.testSet).ok_or_else(|| {
                    crate::Error::Other(format!(
                        "TestSet '{}' not found",
                        ts_ref.testSet
                    ))
                })?;
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
                    kubernetes: if k8s_mocks.is_empty() { None } else { Some(k8s_mocks) },
                    http: if http_mocks.is_empty() { None } else { Some(http_mocks) },
                })
            },
            asserts: if asserts.is_empty() { None } else { Some(asserts) },
        })
    }

    /// Validates that every TestSet referenced in a Test's testSets exists.
    pub fn validate_refs(&self) -> crate::Result<()> {
        for (test_name, test) in &self.tests {
            if let Some(refs) = &test.testSets {
                for ts_ref in refs {
                    if !self.test_sets.contains_key(&ts_ref.testSet) {
                        return Err(crate::Error::Other(
                            format!("Test '{test_name}' references unknown TestSet '{}'", ts_ref.testSet)));
                    }
                }
            }
        }
        Ok(())
    }

    /// Reads all .yml/.yaml files in `dir`, parses multi-doc YAML,
    /// and yields (kind, serde_json::Value, filename) for each document
    /// matching our apiVersion.
    fn scan_yaml_dir(dir: &Path) -> crate::Result<Vec<(String, serde_json::Value, String)>> {
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
            let docs = YamlDoc::from_str_multi(&content)
                .map_err(|e| crate::Error::YamlError(format!("{display}: {e}")))?;
            for doc in docs {
                let (api_version, kind) = match &doc.0 {
                    Value::Mapping(m) => {
                        let av = m.get(&Value::String("apiVersion".into()))
                            .and_then(|v| match v { Value::String(s) => Some(s.clone()), _ => None });
                        let k = m.get(&Value::String("kind".into()))
                            .and_then(|v| match v { Value::String(s) => Some(s.clone()), _ => None });
                        (av, k)
                    }
                    _ => continue,
                };
                if api_version.as_deref() != Some(API_VERSION) {
                    continue;
                }
                if let Some(kind) = kind {
                    let json = yaml_value_to_serde_json(doc.0);
                    results.push((kind, json, display.clone()));
                }
            }
        }
        Ok(results)
    }
}
