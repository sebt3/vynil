use super::vyniltestset::{
    VynilAssert, VynilAssertMatch, VynilAssertResult, VynilAssertSelector, VynilTestSetMocks,
};
use rhai::Dynamic;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

// ── Assert helpers (private) ────────────────────────────────────────────────

/// Checks whether `expected` is a subset of `actual`:
/// every key/value in expected must exist and match in actual.
/// Arrays are compared element-by-element (same length required).
fn json_subset_match(expected: &serde_json::Value, actual: &serde_json::Value) -> bool {
    match (expected, actual) {
        (serde_json::Value::Object(exp), serde_json::Value::Object(act)) => exp
            .iter()
            .all(|(k, v)| act.get(k).is_some_and(|av| json_subset_match(v, av))),
        (serde_json::Value::Array(exp), serde_json::Value::Array(act)) => {
            exp.len() == act.len() && exp.iter().zip(act.iter()).all(|(e, a)| json_subset_match(e, a))
        }
        _ => expected == actual,
    }
}

/// Returns true if a generated Dynamic object matches the selector criteria.
fn matches_selector(d: &Dynamic, selector: &VynilAssertSelector) -> bool {
    let map = match d.as_map_ref() {
        Ok(m) => m,
        Err(_) => return false,
    };
    if let Some(kind) = &selector.kind {
        let actual = map.get("kind").and_then(|v| v.clone().into_string().ok());
        if actual.as_deref() != Some(kind) {
            return false;
        }
    }
    if selector.name.is_none() && selector.namespace.is_none() {
        return true;
    }
    let meta_dyn = match map.get("metadata") {
        Some(m) => m,
        None => return false,
    };
    let meta = match meta_dyn.as_map_ref() {
        Ok(m) => m,
        Err(_) => return false,
    };
    if let Some(name) = &selector.name {
        let actual = meta.get("name").and_then(|v| v.clone().into_string().ok());
        if actual.as_deref() != Some(name) {
            return false;
        }
    }
    if let Some(ns) = &selector.namespace {
        let actual = meta.get("namespace").and_then(|v| v.clone().into_string().ok());
        if actual.as_deref() != Some(ns) {
            return false;
        }
    }
    true
}

impl VynilTest {
    /// Runs all asserts against the generated objects and returns results.
    pub fn run_asserts(&self, generated: &[Dynamic]) -> Vec<VynilAssertResult> {
        let asserts = match &self.asserts {
            Some(a) => a,
            None => return vec![],
        };
        asserts
            .iter()
            .map(|a| {
                let selected: Vec<&Dynamic> = generated
                    .iter()
                    .filter(|d| matches_selector(d, &a.selector))
                    .collect();
                let total = selected.len();
                if total == 0 && !matches!(a.matcher, VynilAssertMatch::None | VynilAssertMatch::AtMost(_)) {
                    return VynilAssertResult {
                        name: a.name.clone(),
                        description: a.description.clone(),
                        passed: false,
                        message: "no objects matched selector".into(),
                    };
                }
                let (passed, message) = if let Some(value) = a.value.clone() {
                    let matching = selected
                        .iter()
                        .filter(|d| {
                            let json: serde_json::Value = serde_json::from_str(
                                &serde_json::to_string(*d).unwrap_or_default(),
                            )
                            .unwrap_or_default();
                            json_subset_match(&value, &json)
                        })
                        .count();
                    match &a.matcher {
                        VynilAssertMatch::All => {
                            if matching == total {
                                (true, format!("{matching}/{total} match"))
                            } else {
                                (false, format!("{matching}/{total} match, expected all"))
                            }
                        }
                        VynilAssertMatch::Any => {
                            if matching > 0 {
                                (true, format!("{matching}/{total} match"))
                            } else {
                                (false, format!("0/{total} match, expected at least one"))
                            }
                        }
                        VynilAssertMatch::Exact(n) => {
                            let n = *n as usize;
                            if matching == n {
                                (true, format!("{matching}/{total} match"))
                            } else {
                                (false, format!("{matching}/{total} match, expected exactly {n}"))
                            }
                        }
                        VynilAssertMatch::AtLeast(n) => {
                            let n = *n as usize;
                            if matching >= n {
                                (true, format!("{matching}/{total} match"))
                            } else {
                                (false, format!("{matching}/{total} match, expected at least {n}"))
                            }
                        }
                        VynilAssertMatch::AtMost(n) => {
                            let n = *n as usize;
                            if matching <= n {
                                (true, format!("{matching}/{total} match"))
                            } else {
                                (false, format!("{matching}/{total} match, expected at most {n}"))
                            }
                        }
                        VynilAssertMatch::None => {
                            if matching == 0 {
                                (true, format!("0/{total} match as expected"))
                            } else {
                                (false, format!("{matching}/{total} match, expected none"))
                            }
                        }
                    }
                } else {
                    match &a.matcher {
                        VynilAssertMatch::All => {
                            if total > 0 {
                                (true, format!("{total} match"))
                            } else {
                                (false, format!("{total} match, expected at least one"))
                            }
                        }
                        VynilAssertMatch::Any => {
                            if total > 0 {
                                (true, format!("{total} match"))
                            } else {
                                (false, format!("{total} match, expected at least one"))
                            }
                        }
                        VynilAssertMatch::Exact(n) => {
                            let n = *n as usize;
                            if total == n {
                                (true, format!("{total} match"))
                            } else {
                                (false, format!("{total} match, expected exactly {n}"))
                            }
                        }
                        VynilAssertMatch::AtLeast(n) => {
                            let n = *n as usize;
                            if total >= n {
                                (true, format!("{total} match"))
                            } else {
                                (false, format!("{total} match, expected at least {n}"))
                            }
                        }
                        VynilAssertMatch::AtMost(n) => {
                            let n = *n as usize;
                            if total <= n {
                                (true, format!("{total} match"))
                            } else {
                                (false, format!("{total} match, expected at most {n}"))
                            }
                        }
                        VynilAssertMatch::None => {
                            if total == 0 {
                                (true, format!("{total} match as expected"))
                            } else {
                                (false, format!("{total} match, expected none"))
                            }
                        }
                    }
                };
                VynilAssertResult {
                    name: a.name.clone(),
                    description: a.description.clone(),
                    passed,
                    message,
                }
            })
            .collect()
    }
}
