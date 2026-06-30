use crate::{RhaiRes, register_k8s_generic, register_k8s_object, register_k8s_raw};
use kube::api::DynamicObject;
use rhai::{Dynamic, Engine, Map, serde::to_dynamic};
use std::sync::{Arc, Mutex};

type DynObjCondition = Box<dyn Fn(&DynamicObject) -> Result<bool, Box<rhai::EvalAltResult>>>;

#[derive(Clone, Debug)]
pub struct K8sObjectMock {
    pub obj: Dynamic,
    pub kind: String,
}
impl K8sObjectMock {
    pub fn rhai_delete(&mut self) -> RhaiRes<()> {
        Ok(())
    }

    pub fn rhai_wait_deleted(&mut self, _timeout: i64) -> RhaiRes<()> {
        Ok(())
    }

    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        if self.obj.is_map()
            && self.obj.as_map_ref().unwrap().contains_key("metadata")
            && self.obj.as_map_ref().unwrap()["metadata"].is_map()
        {
            Ok(self.obj.as_map_ref().unwrap()["metadata"].clone())
        } else {
            Err(format!("Failed to extract metadata from a {}", self.kind).into())
        }
    }

    pub fn get_kind(&mut self) -> String {
        self.kind.clone()
    }

    pub fn is_condition(_cond: String) -> impl kube::runtime::wait::Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| true
    }

    pub fn wait_condition(&mut self, _condition: String, _timeout: i64) -> RhaiRes<()> {
        Ok(())
    }

    pub fn is_status(_prop: String) -> impl kube::runtime::wait::Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| true
    }

    pub fn have_status(_prop: String) -> impl kube::runtime::wait::Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| true
    }

    pub fn have_status_value(
        _prop: String,
        _value: String,
    ) -> impl kube::runtime::wait::Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| true
    }

    pub fn wait_status(&mut self, _prop: String, _timeout: i64) -> RhaiRes<()> {
        Ok(())
    }

    pub fn wait_status_prop(&mut self, _prop: String, _timeout: i64) -> RhaiRes<()> {
        Ok(())
    }

    pub fn wait_status_string(&mut self, _prop: String, _value: String, _timeout: i64) -> RhaiRes<()> {
        Ok(())
    }

    pub fn is_for(_cond: DynObjCondition) -> impl kube::runtime::wait::Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| true
    }

    pub fn wait_for(&mut self, _condition: DynObjCondition, _timeout: i64) -> RhaiRes<()> {
        Ok(())
    }

    pub fn original_kind(&mut self) -> String {
        self.get_kind()
    }
}

// ── K8sRaw mock ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct K8sRawMock;

impl Default for K8sRawMock {
    fn default() -> Self {
        Self::new()
    }
}

impl K8sRawMock {
    pub fn new() -> Self {
        Self
    }

    pub fn rhai_get_url(&mut self, _url: String) -> RhaiRes<Dynamic> {
        to_dynamic(serde_json::Value::Object(Default::default()))
    }

    pub fn rhai_get_api_version(&mut self) -> RhaiRes<Dynamic> {
        to_dynamic(serde_json::Value::Object(Default::default()))
    }

    pub fn rhai_get_api_resources(&mut self) -> RhaiRes<Dynamic> {
        to_dynamic(serde_json::Value::Object(Default::default()))
    }
}

// ── K8sWorkload mock (shared by Deploy, DaemonSet, StatefulSet, Job) ────────

#[derive(Clone, Debug)]
pub struct K8sWorkloadMock {
    pub obj: Dynamic,
}

impl K8sWorkloadMock {
    fn get_sub(&self, key: &str) -> RhaiRes<Dynamic> {
        if self.obj.is_map() {
            let map = self.obj.as_map_ref().unwrap();
            if map.contains_key(key) {
                return Ok(map[key].clone());
            }
        }
        Ok(Dynamic::UNIT)
    }

    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        self.get_sub("metadata")
    }

    pub fn get_spec(&mut self) -> RhaiRes<Dynamic> {
        self.get_sub("spec")
    }

    pub fn get_status(&mut self) -> RhaiRes<Dynamic> {
        self.get_sub("status")
    }

    pub fn wait_available(&mut self, _timeout: i64) -> RhaiRes<()> {
        Ok(())
    }

    pub fn wait_done(&mut self, _timeout: i64) -> RhaiRes<()> {
        Ok(())
    }
}

fn find_workload_mock(
    mocks: Arc<Mutex<Vec<Dynamic>>>,
    kind: &str,
    namespace: &str,
    name: &str,
) -> RhaiRes<K8sWorkloadMock> {
    for m in mocks.lock().unwrap().clone() {
        if !m.is_map() {
            continue;
        }
        let map = m.as_map_ref().unwrap();
        if !map.contains_key("kind") || !map["kind"].is_string() {
            continue;
        }
        if map["kind"].clone().into_string().unwrap() != kind {
            continue;
        }
        if !map.contains_key("metadata") || !map["metadata"].is_map() {
            continue;
        }
        let meta = map["metadata"].as_map_ref().unwrap();
        let name_match = meta.contains_key("name")
            && meta["name"].is_string()
            && meta["name"].clone().into_string().unwrap() == name;
        let ns_match = meta.contains_key("namespace")
            && meta["namespace"].is_string()
            && meta["namespace"].clone().into_string().unwrap() == namespace;
        if name_match && ns_match {
            return Ok(K8sWorkloadMock { obj: m.clone() });
        }
    }
    Err(format!("Failed to find {kind} {name} in namespace {namespace} in the Mock database").into())
}

fn deep_merge_dynamic(base: Dynamic, patch: &Dynamic) -> Dynamic {
    if base.is_map() && patch.is_map() {
        let mut merged: Map = base.as_map_ref().unwrap().clone();
        for (k, v) in patch.as_map_ref().unwrap().iter() {
            let new_val = if let Some(existing) = merged.get(k.as_str()) {
                deep_merge_dynamic(existing.clone(), v)
            } else {
                v.clone()
            };
            merged.insert(k.clone(), new_val);
        }
        Dynamic::from_map(merged)
    } else {
        patch.clone()
    }
}

fn merge_with_existing(list: &[Dynamic], kind: &str, obj: &Dynamic) -> Dynamic {
    if !obj.is_map() {
        return obj.clone();
    }
    let map = obj.as_map_ref().unwrap();
    let meta: Option<Map> = match map.get("metadata") {
        Some(m) if m.is_map() => Some(m.as_map_ref().unwrap().clone()),
        _ => None,
    };
    let obj_name = meta
        .as_ref()
        .and_then(|m| m.get("name"))
        .and_then(|n| n.clone().into_string().ok());
    let obj_ns = meta
        .as_ref()
        .and_then(|m| m.get("namespace"))
        .and_then(|n| n.clone().into_string().ok());
    for entry in list.iter() {
        if !entry.is_map() {
            continue;
        }
        let entry_map: Map = entry.as_map_ref().unwrap().clone();
        let entry_kind = entry_map.get("kind").and_then(|k| k.clone().into_string().ok());
        if entry_kind.as_deref() != Some(kind) {
            continue;
        }
        let entry_meta: Option<Map> = match entry_map.get("metadata") {
            Some(m) if m.is_map() => Some(m.as_map_ref().unwrap().clone()),
            _ => None,
        };
        let entry_name = entry_meta
            .as_ref()
            .and_then(|m| m.get("name"))
            .and_then(|n| n.clone().into_string().ok());
        let entry_ns = entry_meta
            .as_ref()
            .and_then(|m| m.get("namespace"))
            .and_then(|n| n.clone().into_string().ok());
        if entry_name == obj_name && entry_ns == obj_ns {
            return deep_merge_dynamic(entry.clone(), obj);
        }
    }
    obj.clone()
}

fn upsert_in_list(list: &mut Vec<Dynamic>, kind: &str, obj: &Dynamic) {
    if !obj.is_map() {
        list.push(obj.clone());
        return;
    }
    let map = obj.as_map_ref().unwrap();
    let meta: Option<Map> = match map.get("metadata") {
        Some(m) if m.is_map() => Some(m.as_map_ref().unwrap().clone()),
        _ => None,
    };
    let obj_name = meta
        .as_ref()
        .and_then(|m| m.get("name"))
        .and_then(|n| n.clone().into_string().ok());
    let obj_ns = meta
        .as_ref()
        .and_then(|m| m.get("namespace"))
        .and_then(|n| n.clone().into_string().ok());
    for entry in list.iter_mut() {
        if !entry.is_map() {
            continue;
        }
        let entry_map: Map = entry.as_map_ref().unwrap().clone();
        let entry_kind = entry_map.get("kind").and_then(|k| k.clone().into_string().ok());
        if entry_kind.as_deref() != Some(kind) {
            continue;
        }
        let entry_meta: Option<Map> = match entry_map.get("metadata") {
            Some(m) if m.is_map() => Some(m.as_map_ref().unwrap().clone()),
            _ => None,
        };
        let entry_name = entry_meta
            .as_ref()
            .and_then(|m| m.get("name"))
            .and_then(|n| n.clone().into_string().ok());
        let entry_ns = entry_meta
            .as_ref()
            .and_then(|m| m.get("namespace"))
            .and_then(|n| n.clone().into_string().ok());
        if entry_name == obj_name && entry_ns == obj_ns {
            *entry = deep_merge_dynamic(entry.clone(), obj);
            return;
        }
    }
    list.push(obj.clone());
}

// ── K8sGenericMock ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct K8sGenericMock {
    pub kind: String,
    pub ns: Option<String>,
    pub my_mocks: Vec<Dynamic>,
    pub mocks: Arc<Mutex<Vec<Dynamic>>>,
    pub created: Arc<Mutex<Vec<Dynamic>>>,
}

impl K8sGenericMock {
    #[must_use]
    pub fn new(
        mocks: Arc<Mutex<Vec<Dynamic>>>,
        name: &str,
        ns: Option<String>,
        created: Arc<Mutex<Vec<Dynamic>>>,
    ) -> Self {
        let my_mocks: Vec<Dynamic> = mocks
            .lock()
            .unwrap()
            .clone()
            .into_iter()
            .filter(|m| {
                m.is_map()
                    && m.as_map_ref().unwrap().contains_key("kind")
                    && m.as_map_ref().unwrap()["kind"].is_string()
                    && m.as_map_ref().unwrap()["kind"].clone().into_string().unwrap() == name
            })
            .collect();
        if ns.is_some() {
            Self {
                kind: name.into(),
                ns: ns.clone(),
                mocks: mocks.clone(),
                my_mocks: my_mocks
                    .into_iter()
                    .filter(|m| {
                        m.is_map()
                            && m.as_map_ref().unwrap().contains_key("metadata")
                            && m.as_map_ref().unwrap()["metadata"].is_map()
                            && m.as_map_ref().unwrap()["metadata"]
                                .as_map_ref()
                                .unwrap()
                                .contains_key("namespace")
                            && m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["namespace"]
                                .is_string()
                            && m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["namespace"]
                                .clone()
                                .into_string()
                                .unwrap()
                                == ns.clone().unwrap()
                    })
                    .collect(),
                created,
            }
        } else {
            Self {
                kind: name.into(),
                ns,
                mocks,
                my_mocks,
                created,
            }
        }
    }

    #[must_use]
    pub fn new_api_version(
        mocks: Arc<Mutex<Vec<Dynamic>>>,
        _api_group: &str,
        _version: &str,
        name: &str,
        ns: Option<String>,
        created: Arc<Mutex<Vec<Dynamic>>>,
    ) -> Self {
        Self::new(mocks, name, ns, created)
    }

    pub fn new_ns(
        mocks: Arc<Mutex<Vec<Dynamic>>>,
        name: String,
        ns: String,
        created: Arc<Mutex<Vec<Dynamic>>>,
    ) -> Self {
        Self::new(mocks, name.as_str(), Some(ns), created)
    }

    pub fn new_global(
        mocks: Arc<Mutex<Vec<Dynamic>>>,
        name: String,
        created: Arc<Mutex<Vec<Dynamic>>>,
    ) -> Self {
        Self::new(mocks, name.as_str(), None, created)
    }

    pub fn new_group_ns(
        mocks: Arc<Mutex<Vec<Dynamic>>>,
        api_version: String,
        name: String,
        ns: String,
        created: Arc<Mutex<Vec<Dynamic>>>,
    ) -> Self {
        let arr = api_version.split("/").collect::<Vec<&str>>();
        if arr.len() > 1 {
            Self::new_api_version(mocks, arr[0], arr[1], name.as_str(), Some(ns), created)
        } else {
            Self::new(mocks, name.as_str(), Some(ns), created)
        }
    }

    pub fn rhai_get_scope(&mut self) -> String {
        "namespace".to_string()
    }

    pub fn rhai_exist(&mut self) -> RhaiRes<Dynamic> {
        Ok(true.into())
    }

    pub fn rhai_list(&mut self) -> RhaiRes<Dynamic> {
        to_dynamic(serde_json::json!({"items": self.my_mocks.clone()}))
    }

    pub fn rhai_list_labels(&mut self, _labels: String) -> RhaiRes<Dynamic> {
        to_dynamic(serde_json::json!({"items": self.my_mocks.clone()}))
    }

    pub fn rhai_list_meta(&mut self) -> RhaiRes<Dynamic> {
        self.rhai_list()
    }

    fn meta_name(m: &Dynamic) -> Option<String> {
        let map = m.as_map_ref().ok()?;
        let meta = map.get("metadata")?.clone();
        let mm = meta.as_map_ref().ok()?;
        mm.get("name")?.clone().into_string().ok()
    }

    fn find_by_name_in(items: &[Dynamic], name: &str) -> Option<Dynamic> {
        items
            .iter()
            .find(|m| Self::meta_name(m).as_deref() == Some(name))
            .cloned()
    }

    fn find_by_name_in_live(&self, name: &str) -> Option<Dynamic> {
        let mocks = self.mocks.lock().unwrap();
        mocks
            .iter()
            .find(|m| {
                let Ok(map) = m.as_map_ref() else { return false };
                let kind_ok = map
                    .get("kind")
                    .and_then(|k| k.clone().into_string().ok())
                    .as_deref()
                    == Some(self.kind.as_str());
                let meta_dyn = match map.get("metadata").cloned() {
                    Some(v) => v,
                    None => return false,
                };
                let Ok(meta) = meta_dyn.as_map_ref() else {
                    return false;
                };
                let name_ok = meta
                    .get("name")
                    .and_then(|n: &Dynamic| n.clone().into_string().ok())
                    .as_deref()
                    == Some(name);
                let ns_ok = self.ns.as_ref().is_none_or(|ns| {
                    meta.get("namespace")
                        .and_then(|n: &Dynamic| n.clone().into_string().ok())
                        .as_deref()
                        == Some(ns.as_str())
                });
                kind_ok && name_ok && ns_ok
            })
            .cloned()
    }

    pub fn rhai_get(&mut self, name: String) -> RhaiRes<Dynamic> {
        if let Some(obj) =
            Self::find_by_name_in(&self.my_mocks, &name).or_else(|| self.find_by_name_in_live(&name))
        {
            Ok(obj)
        } else {
            Err(format!("Failed to find {} {name} in the Mock database", self.kind).into())
        }
    }

    pub fn rhai_get_meta(&mut self, name: String) -> RhaiRes<Dynamic> {
        self.rhai_get(name)
    }

    pub fn rhai_get_obj(&mut self, name: String) -> RhaiRes<K8sObjectMock> {
        if let Some(obj) =
            Self::find_by_name_in(&self.my_mocks, &name).or_else(|| self.find_by_name_in_live(&name))
        {
            Ok(K8sObjectMock {
                obj,
                kind: self.kind.clone(),
            })
        } else {
            Err(format!("Failed to find {} {name} in the Mock database", self.kind).into())
        }
    }

    pub fn rhai_delete(&mut self, _name: String) -> RhaiRes<()> {
        Ok(())
    }

    pub fn rhai_apply(&mut self, _name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        let mut obj = data;
        if let Some(ns) = self.ns.clone()
            && obj.is_map()
            && obj.as_map_ref().unwrap().contains_key("metadata")
            && obj.as_map_ref().unwrap()["metadata"].is_map()
            && !obj.as_map_ref().unwrap()["metadata"]
                .as_map_ref()
                .unwrap()
                .contains_key("namespace")
        {
            obj.as_map_mut()
                .unwrap()
                .entry("metadata".into())
                .and_modify(|meta| {
                    meta.as_map_mut().unwrap().insert("namespace".into(), ns.into());
                });
        }
        if obj.is_map() && !obj.as_map_ref().unwrap().contains_key("kind") {
            obj.as_map_mut()
                .unwrap()
                .insert("kind".into(), Dynamic::from(self.kind.clone()));
        }
        let merged = merge_with_existing(&self.mocks.lock().unwrap(), &self.kind, &obj);
        upsert_in_list(&mut self.created.lock().unwrap(), &self.kind, &merged);
        upsert_in_list(&mut self.mocks.lock().unwrap(), &self.kind, &obj);
        Ok(obj)
    }

    pub fn rhai_replace(&mut self, _name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        let mut obj = data;
        if obj.is_map() && !obj.as_map_ref().unwrap().contains_key("kind") {
            obj.as_map_mut()
                .unwrap()
                .insert("kind".into(), Dynamic::from(self.kind.clone()));
        }
        let merged = merge_with_existing(&self.mocks.lock().unwrap(), &self.kind, &obj);
        upsert_in_list(&mut self.created.lock().unwrap(), &self.kind, &merged);
        upsert_in_list(&mut self.mocks.lock().unwrap(), &self.kind, &obj);
        Ok(obj)
    }

    pub fn rhai_patch(&mut self, _name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        let mut obj = data;
        if obj.is_map() && !obj.as_map_ref().unwrap().contains_key("kind") {
            obj.as_map_mut()
                .unwrap()
                .insert("kind".into(), Dynamic::from(self.kind.clone()));
        }
        upsert_in_list(&mut self.mocks.lock().unwrap(), &self.kind, &obj);
        Ok(obj)
    }

    pub fn rhai_create(&mut self, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        let mut obj = data;
        if obj.is_map() && !obj.as_map_ref().unwrap().contains_key("kind") {
            obj.as_map_mut()
                .unwrap()
                .insert("kind".into(), Dynamic::from(self.kind.clone()));
        }
        let merged = merge_with_existing(&self.mocks.lock().unwrap(), &self.kind, &obj);
        upsert_in_list(&mut self.created.lock().unwrap(), &self.kind, &merged);
        upsert_in_list(&mut self.mocks.lock().unwrap(), &self.kind, &obj);
        Ok(obj)
    }
}

// ── Rhai registration (generic part only) ────────────────────────────────────

pub fn k8s_mock_rhai_register(
    engine: &mut Engine,
    arced_mocks: Arc<Mutex<Vec<Dynamic>>>,
    created: Arc<Mutex<Vec<Dynamic>>>,
) {
    let lmocks = arced_mocks.clone();
    let lcreated = created.clone();
    let new_global = move |name: String| -> K8sGenericMock {
        let mock = lmocks.clone();
        K8sGenericMock::new_global(mock.clone(), name, lcreated.clone())
    };
    let lmocks = arced_mocks.clone();
    let lcreated = created.clone();
    let new_ns = move |name: String, ns: String| -> K8sGenericMock {
        let mock = lmocks.clone();
        K8sGenericMock::new_ns(mock, name, ns, lcreated.clone())
    };
    let lmocks = arced_mocks.clone();
    let lcreated = created.clone();
    let new_group_ns = move |apiv: String, name: String, ns: String| -> K8sGenericMock {
        let mock = lmocks.clone();
        K8sGenericMock::new_group_ns(mock, apiv, name, ns, lcreated.clone())
    };
    engine
        .register_type_with_name::<DynamicObject>("DynamicObject")
        .register_get("data", |obj: &mut DynamicObject| -> Dynamic {
            Dynamic::from(obj.data.clone())
        });
    register_k8s_object!(engine, K8sObjectMock);
    register_k8s_generic!(
        engine,
        K8sGenericMock,
        K8sObjectMock,
        new_global,
        new_ns,
        new_group_ns
    );

    register_k8s_raw!(engine, K8sRawMock, K8sRawMock::new);

    // K8sDeploy
    let wl_mocks = arced_mocks.clone();
    engine
        .register_type_with_name::<K8sWorkloadMock>("K8sDeploy")
        .register_fn(
            "get_deployment",
            move |ns: String, name: String| -> RhaiRes<K8sWorkloadMock> {
                let mock = wl_mocks.clone();
                find_workload_mock(mock, "Deployment", &ns, &name)
            },
        )
        .register_get("metadata", K8sWorkloadMock::get_metadata)
        .register_get("spec", K8sWorkloadMock::get_spec)
        .register_get("status", K8sWorkloadMock::get_status)
        .register_fn("wait_available", K8sWorkloadMock::wait_available);

    // K8sDaemonSet
    let wl_mocks = arced_mocks.clone();
    engine
        .register_type_with_name::<K8sWorkloadMock>("K8sDaemonSet")
        .register_fn(
            "get_deamonset",
            move |ns: String, name: String| -> RhaiRes<K8sWorkloadMock> {
                let mock = wl_mocks.clone();
                find_workload_mock(mock, "DaemonSet", &ns, &name)
            },
        )
        .register_get("metadata", K8sWorkloadMock::get_metadata)
        .register_get("spec", K8sWorkloadMock::get_spec)
        .register_get("status", K8sWorkloadMock::get_status)
        .register_fn("wait_available", K8sWorkloadMock::wait_available);

    // K8sStatefulSet
    let wl_mocks = arced_mocks.clone();
    engine
        .register_type_with_name::<K8sWorkloadMock>("K8sStatefulSet")
        .register_fn(
            "get_statefulset",
            move |ns: String, name: String| -> RhaiRes<K8sWorkloadMock> {
                let mock = wl_mocks.clone();
                find_workload_mock(mock, "StatefulSet", &ns, &name)
            },
        )
        .register_get("metadata", K8sWorkloadMock::get_metadata)
        .register_get("spec", K8sWorkloadMock::get_spec)
        .register_get("status", K8sWorkloadMock::get_status)
        .register_fn("wait_available", K8sWorkloadMock::wait_available);

    // K8sJob
    let wl_mocks = arced_mocks.clone();
    engine
        .register_type_with_name::<K8sWorkloadMock>("K8sJob")
        .register_fn(
            "get_job",
            move |ns: String, name: String| -> RhaiRes<K8sWorkloadMock> {
                let mock = wl_mocks.clone();
                find_workload_mock(mock, "Job", &ns, &name)
            },
        )
        .register_get("metadata", K8sWorkloadMock::get_metadata)
        .register_get("spec", K8sWorkloadMock::get_spec)
        .register_get("status", K8sWorkloadMock::get_status)
        .register_fn("wait_done", K8sWorkloadMock::wait_done);
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn k8sobjectmock_original_kind_exists() {
        let mut obj = K8sObjectMock {
            obj: rhai::Dynamic::UNIT,
            kind: "Pod".to_string(),
        };
        let _: String = obj.original_kind();
    }

    #[test]
    fn register_k8s_object_mock_compiles() {
        let mut engine = rhai::Engine::new();
        register_k8s_object!(engine, K8sObjectMock);
    }

    #[test]
    fn register_k8s_raw_mock_compiles() {
        let mut engine = rhai::Engine::new();
        register_k8s_raw!(engine, K8sRawMock, K8sRawMock::new);
    }

    #[test]
    fn register_k8s_generic_mock_compiles() {
        use std::sync::{Arc, Mutex};
        let mocks: Arc<Mutex<Vec<rhai::Dynamic>>> = Arc::new(Mutex::new(vec![]));
        let created: Arc<Mutex<Vec<rhai::Dynamic>>> = Arc::new(Mutex::new(vec![]));
        let m1 = mocks.clone();
        let c1 = created.clone();
        let new_global = move |name: String| K8sGenericMock::new_global(m1.clone(), name, c1.clone());
        let m2 = mocks.clone();
        let c2 = created.clone();
        let new_ns = move |n: String, ns: String| K8sGenericMock::new_ns(m2.clone(), n, ns, c2.clone());
        let m3 = mocks.clone();
        let c3 = created.clone();
        let new_group_ns = move |a: String, n: String, ns: String| {
            K8sGenericMock::new_group_ns(m3.clone(), a, n, ns, c3.clone())
        };
        let mut engine = rhai::Engine::new();
        register_k8s_generic!(
            engine,
            K8sGenericMock,
            K8sObjectMock,
            new_global,
            new_ns,
            new_group_ns
        );
    }
}
