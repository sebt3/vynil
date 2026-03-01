use crate::RhaiRes;
use kube::{api::DynamicObject, runtime::wait::Condition};
use rhai::{Dynamic, Engine, Map, serde::to_dynamic};
use serde::{Serialize,Deserialize};
use std::sync::{Arc, Mutex};

pub fn update_cache() {}

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

    pub fn is_condition(_cond: String) -> impl Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| true
    }

    pub fn wait_condition(&mut self, _condition: String, _timeout: i64) -> RhaiRes<()> {
        Ok(())
    }

    pub fn is_status(_prop: String) -> impl Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| true
    }

    pub fn have_status(_prop: String) -> impl Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| true
    }

    pub fn have_status_value(_prop: String, _value: String) -> impl Condition<DynamicObject> {
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

    pub fn is_for(
        _cond: Box<dyn Fn(&DynamicObject) -> Result<bool, Box<rhai::EvalAltResult>>>,
    ) -> impl Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| true
    }

    pub fn wait_for(
        &mut self,
        _condition: Box<dyn Fn(&DynamicObject) -> Result<bool, Box<rhai::EvalAltResult>>>,
        _timeout: i64,
    ) -> RhaiRes<()> {
        Ok(())
    }
}


// ── K8sRaw mock ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct K8sRawMock;

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
        // Check kind
        if !map.contains_key("kind") || !map["kind"].is_string() {
            continue;
        }
        if map["kind"].clone().into_string().unwrap() != kind {
            continue;
        }
        // Check metadata.name and metadata.namespace
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
        let my_mocks: Vec<Dynamic> = mocks.lock().unwrap().clone()
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

    pub fn new_ns(mocks: Arc<Mutex<Vec<Dynamic>>>, name: String, ns: String, created: Arc<Mutex<Vec<Dynamic>>>) -> Self {
        Self::new(mocks, name.as_str(), Some(ns), created)
    }

    pub fn new_global(mocks: Arc<Mutex<Vec<Dynamic>>>, name: String, created: Arc<Mutex<Vec<Dynamic>>>) -> Self {
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
        //TODO : to the label filtering
        to_dynamic(serde_json::json!({"items": self.my_mocks.clone()}))
    }

    pub fn rhai_list_meta(&mut self) -> RhaiRes<Dynamic> {
        self.rhai_list()
    }

    pub fn rhai_get(&mut self, name: String) -> RhaiRes<Dynamic> {
        let found: Vec<Dynamic> = self
            .my_mocks
            .clone()
            .into_iter()
            .filter(|m| {
                m.is_map()
                    && m.as_map_ref().unwrap().contains_key("metadata")
                    && m.as_map_ref().unwrap()["metadata"].is_map()
                    && m.as_map_ref().unwrap()["metadata"]
                        .as_map_ref()
                        .unwrap()
                        .contains_key("name")
                    && m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["name"].is_string()
                    && m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["name"]
                        .clone()
                        .into_string()
                        .unwrap()
                        == name
            })
            .collect();
        if found.len() > 0 {
            Ok(found[0].clone())
        } else {
            Err(format!("Failed to find {} {name} in the Mock database", self.kind).into())
        }
    }

    pub fn rhai_get_meta(&mut self, name: String) -> RhaiRes<Dynamic> {
        self.rhai_get(name)
    }

    pub fn rhai_get_obj(&mut self, name: String) -> RhaiRes<K8sObjectMock> {
        let found: Vec<Dynamic> = self
            .my_mocks
            .clone()
            .into_iter()
            .filter(|m| {
                m.is_map()
                    && m.as_map_ref().unwrap().contains_key("metadata")
                    && m.as_map_ref().unwrap()["metadata"].is_map()
                    && m.as_map_ref().unwrap()["metadata"]
                        .as_map_ref()
                        .unwrap()
                        .contains_key("name")
                    && m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["name"].is_string()
                    && m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["name"]
                        .clone()
                        .into_string()
                        .unwrap()
                        == name
            })
            .collect();
        if found.len() > 0 {
            Ok(K8sObjectMock {
                obj: found[0].clone(),
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
        //TODO : Look if the object exist already. Merge if so
        let mut obj = data;
        if let Some(ns) = self.ns.clone() &&
            obj.is_map() &&
            obj.as_map_ref().unwrap().contains_key("metadata") &&
            obj.as_map_ref().unwrap()["metadata"].is_map() &&
            ! obj.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap().contains_key("namespace") {
            obj.as_map_mut().unwrap().entry("metadata".into()).and_modify(|meta| {
                meta.as_map_mut().unwrap().insert("namespace".into(), ns.into());
            });
        }
        self.created.lock().unwrap().push(obj.clone());
        self.mocks.lock().unwrap().push(obj.clone());
        Ok(obj)
    }

    pub fn rhai_replace(&mut self, _name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        self.created.lock().unwrap().push(data.clone());
        //TODO : replace the objet in the mocks DB for real
        Ok(data)
    }

    pub fn rhai_patch(&mut self, _name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        //TODO : update the objet in the mocks DB for real
        Ok(data)
    }

    // Collected for asserts
    pub fn rhai_create(&mut self, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        self.created.lock().unwrap().push(data.clone());
        self.mocks.lock().unwrap().push(data.clone());
        Ok(data)
    }

}

// ── K8sInstance mock (ServiceInstance, SystemInstance, TenantInstance) ────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct K8sInstanceMock {
    pub obj: Dynamic,
}

impl K8sInstanceMock {
    fn get_sub(&self, key: &str) -> RhaiRes<Dynamic> {
        if self.obj.is_map() {
            let map = self.obj.as_map_ref().unwrap();
            if map.contains_key(key) {
                return Ok(map[key].clone());
            }
        }
        Ok(Dynamic::UNIT)
    }

    fn set_status_field(&mut self, key: &str, val: Dynamic) {
        if !self.obj.is_map() {
            return;
        }
        let mut top: Map = self.obj.as_map_ref().unwrap().clone();
        let status = top
            .entry("status".into())
            .or_insert_with(|| Dynamic::from_map(Map::new()));
        if status.is_map() {
            let mut status_map: Map = status.as_map_ref().unwrap().clone();
            status_map.insert(key.into(), val);
            *status = Dynamic::from_map(status_map);
        }
        self.obj = Dynamic::from_map(top);
    }

    // ── Getters ─────────────────────────────────────────────────────────

    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        self.get_sub("metadata")
    }

    pub fn get_spec(&mut self) -> RhaiRes<Dynamic> {
        self.get_sub("spec")
    }

    pub fn get_status(&mut self) -> RhaiRes<Dynamic> {
        self.get_sub("status")
    }

    pub fn get_options_digest(&mut self) -> String {
        String::new()
    }

    pub fn get_tfstate(&mut self) -> RhaiRes<String> {
        let status = self.get_sub("status")?;
        if let Ok(m) = status.as_map_ref() {
            if let Some(v) = m.get("tfstate") {
                if let Ok(s) = v.clone().into_string() {
                    return Ok(s);
                }
            }
        }
        Ok(String::new())
    }

    pub fn get_rhaistate(&mut self) -> RhaiRes<String> {
        let status = self.get_sub("status")?;
        if let Ok(m) = status.as_map_ref() {
            if let Some(v) = m.get("rhaistate") {
                if let Ok(s) = v.clone().into_string() {
                    return Ok(s);
                }
            }
        }
        Ok(String::new())
    }

    // ── Common status setters ───────────────────────────────────────────

    pub fn set_status_ready(&mut self, tag: String) -> RhaiRes<Self> {
        self.set_status_field("tag", Dynamic::from(tag));
        Ok(self.clone())
    }

    pub fn set_agent_started(&mut self) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_missing_box(&mut self, _jukebox: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_missing_package(&mut self, _cat: String, _pkg: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_missing_requirement(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_tfstate(&mut self, tfstate: String) -> RhaiRes<Self> {
        self.set_status_field("tfstate", Dynamic::from(tfstate));
        Ok(self.clone())
    }

    pub fn set_status_tofu_failed(&mut self, _tfstate: String, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_rhaistate(&mut self, rhaistate: String) -> RhaiRes<Self> {
        self.set_status_field("rhaistate", Dynamic::from(rhaistate));
        Ok(self.clone())
    }

    pub fn set_status_rhai_failed(&mut self, _rhaistate: String, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    // ── Children status setters ─────────────────────────────────────────

    pub fn set_status_crds(&mut self, list: Dynamic) -> RhaiRes<Self> {
        self.set_status_field("crds", list);
        Ok(self.clone())
    }

    pub fn set_status_crd_failed(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_status_befores(&mut self, list: Dynamic) -> RhaiRes<Self> {
        self.set_status_field("befores", list);
        Ok(self.clone())
    }

    pub fn set_status_before_failed(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_status_vitals(&mut self, list: Dynamic) -> RhaiRes<Self> {
        self.set_status_field("vitals", list);
        Ok(self.clone())
    }

    pub fn set_status_vital_failed(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_status_scalables(&mut self, list: Dynamic) -> RhaiRes<Self> {
        self.set_status_field("scalables", list);
        Ok(self.clone())
    }

    pub fn set_status_scalable_failed(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_status_others(&mut self, list: Dynamic) -> RhaiRes<Self> {
        self.set_status_field("others", list);
        Ok(self.clone())
    }

    pub fn set_status_other_failed(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_status_posts(&mut self, list: Dynamic) -> RhaiRes<Self> {
        self.set_status_field("posts", list);
        Ok(self.clone())
    }

    pub fn set_status_post_failed(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_status_systems(&mut self, list: Dynamic) -> RhaiRes<Self> {
        self.set_status_field("systems", list);
        Ok(self.clone())
    }

    pub fn set_status_system_failed(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_status_init_failed(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    pub fn set_status_schedule_backup_failed(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }

    // ── Services ────────────────────────────────────────────────────────

    pub fn set_services(&mut self, services: Dynamic) -> RhaiRes<Self> {
        self.set_status_field("services", services);
        Ok(self.clone())
    }

    pub fn get_services_string(&mut self) -> String {
        if let Ok(status) = self.get_sub("status") {
            if let Ok(status_map) = status.as_map_ref() {
                if let Some(services) = status_map.get("services") {
                    if let Ok(arr) = services.clone().into_array() {
                        let mut keys: Vec<String> = arr
                            .iter()
                            .filter_map(|s| {
                                let m = s.as_map_ref().ok()?;
                                let k = m.get("key")?;
                                k.clone().into_string().ok()
                            })
                            .collect();
                        keys.sort();
                        return keys.join(",");
                    }
                }
            }
        }
        String::new()
    }

    // ── Tenant-specific ─────────────────────────────────────────────────

    pub fn get_tenant_name(&mut self) -> RhaiRes<String> {
        if let Ok(meta) = self.get_sub("metadata") {
            if let Ok(m) = meta.as_map_ref() {
                if let Some(ns) = m.get("namespace") {
                    if let Ok(s) = ns.clone().into_string() {
                        return Ok(s);
                    }
                }
            }
        }
        Ok(String::new())
    }

    pub fn get_tenant_namespaces(&mut self) -> RhaiRes<Dynamic> {
        let ns = self.get_tenant_name()?;
        Ok(Dynamic::from_array(vec![Dynamic::from(ns)]))
    }

    pub fn get_tenant_services_names(&mut self) -> RhaiRes<Dynamic> {
        Ok(Dynamic::from_array(vec![]))
    }
}

fn find_instance_mock(
    mocks: Arc<Mutex<Vec<Dynamic>>>,
    kind: &str,
    namespace: &str,
    name: &str,
) -> RhaiRes<K8sInstanceMock> {
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
            return Ok(K8sInstanceMock { obj: m.clone() });
        }
    }
    Err(format!("Failed to find {kind} {name} in namespace {namespace} in the Mock database").into())
}

fn list_instance_mocks(mocks: Arc<Mutex<Vec<Dynamic>>>, kind: &str, namespace: &str) -> RhaiRes<Dynamic> {
    let items: Vec<K8sInstanceMock> = mocks.lock().unwrap().clone()
        .iter()
        .filter(|m| {
            if !m.is_map() {
                return false;
            }
            let map = m.as_map_ref().unwrap();
            if !map.contains_key("kind") || !map["kind"].is_string() {
                return false;
            }
            if map["kind"].clone().into_string().unwrap() != kind {
                return false;
            }
            if !map.contains_key("metadata") || !map["metadata"].is_map() {
                return false;
            }
            let meta = map["metadata"].as_map_ref().unwrap();
            meta.contains_key("namespace")
                && meta["namespace"].is_string()
                && meta["namespace"].clone().into_string().unwrap() == namespace
        })
        .map(|m| K8sInstanceMock { obj: m.clone() })
        .collect();
    to_dynamic(serde_json::json!({"items": items}))
}

// ── JukeBox mock ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct K8sJukeBoxMock {
    pub obj: Dynamic,
}

impl K8sJukeBoxMock {
    fn get_sub(&self, key: &str) -> RhaiRes<Dynamic> {
        if self.obj.is_map() {
            let map = self.obj.as_map_ref().unwrap();
            if map.contains_key(key) {
                return Ok(map[key].clone());
            }
        }
        Ok(Dynamic::UNIT)
    }

    fn set_status_field(&mut self, key: &str, val: Dynamic) {
        if !self.obj.is_map() {
            return;
        }
        let mut top: Map = self.obj.as_map_ref().unwrap().clone();
        let status = top
            .entry("status".into())
            .or_insert_with(|| Dynamic::from_map(Map::new()));
        if status.is_map() {
            let mut status_map: Map = status.as_map_ref().unwrap().clone();
            status_map.insert(key.into(), val);
            *status = Dynamic::from_map(status_map);
        }
        self.obj = Dynamic::from_map(top);
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

    pub fn set_status_updated(&mut self, packages: Dynamic) -> RhaiRes<Self> {
        self.set_status_field("packages", packages);
        Ok(self.clone())
    }

    pub fn set_status_failed(&mut self, _reason: String) -> RhaiRes<Self> {
        Ok(self.clone())
    }
}

fn find_jukebox_mock(mocks: Arc<Mutex<Vec<Dynamic>>>, name: &str) -> RhaiRes<K8sJukeBoxMock> {
    for m in mocks.lock().unwrap().clone() {
        if !m.is_map() {
            continue;
        }
        let map = m.as_map_ref().unwrap();
        if !map.contains_key("kind") || !map["kind"].is_string() {
            continue;
        }
        if map["kind"].clone().into_string().unwrap() != "JukeBox" {
            continue;
        }
        if !map.contains_key("metadata") || !map["metadata"].is_map() {
            continue;
        }
        let meta = map["metadata"].as_map_ref().unwrap();
        if meta.contains_key("name")
            && meta["name"].is_string()
            && meta["name"].clone().into_string().unwrap() == name
        {
            return Ok(K8sJukeBoxMock { obj: m.clone() });
        }
    }
    Err(format!("Failed to find JukeBox {name} in the Mock database").into())
}

fn list_jukebox_mocks(mocks: Arc<Mutex<Vec<Dynamic>>>) -> RhaiRes<Dynamic> {
    let items: Vec<K8sJukeBoxMock> = mocks.lock().unwrap().clone()
        .iter()
        .filter(|m| {
            if !m.is_map() {
                return false;
            }
            let map = m.as_map_ref().unwrap();
            if !map.contains_key("kind") || !map["kind"].is_string() {
                return false;
            }
            map["kind"].clone().into_string().unwrap() == "JukeBox"
        })
        .map(|m| K8sJukeBoxMock { obj: m.clone() })
        .collect();
    to_dynamic(serde_json::json!({"items": items}))
}

fn register_instance_common(engine: &mut Engine) {
    engine
        .register_fn("options_digest", K8sInstanceMock::get_options_digest)
        .register_fn("get_tfstate", K8sInstanceMock::get_tfstate)
        .register_fn("get_rhaistate", K8sInstanceMock::get_rhaistate)
        .register_fn("set_agent_started", K8sInstanceMock::set_agent_started)
        .register_fn("set_missing_box", K8sInstanceMock::set_missing_box)
        .register_fn("set_missing_package", K8sInstanceMock::set_missing_package)
        .register_fn(
            "set_missing_requirement",
            K8sInstanceMock::set_missing_requirement,
        )
        .register_fn("set_status_ready", K8sInstanceMock::set_status_ready)
        .register_fn("set_tfstate", K8sInstanceMock::set_tfstate)
        .register_fn("set_status_tofu_failed", K8sInstanceMock::set_status_tofu_failed)
        .register_fn("set_rhaistate", K8sInstanceMock::set_rhaistate)
        .register_fn("set_status_rhai_failed", K8sInstanceMock::set_status_rhai_failed)
        .register_get("metadata", K8sInstanceMock::get_metadata)
        .register_get("spec", K8sInstanceMock::get_spec)
        .register_get("status", K8sInstanceMock::get_status);
}

fn register_instance_children(engine: &mut Engine) {
    engine
        .register_fn("set_services", K8sInstanceMock::set_services)
        .register_fn("get_services", K8sInstanceMock::get_services_string)
        .register_fn("set_status_befores", K8sInstanceMock::set_status_befores)
        .register_fn(
            "set_status_before_failed",
            K8sInstanceMock::set_status_before_failed,
        )
        .register_fn("set_status_vitals", K8sInstanceMock::set_status_vitals)
        .register_fn(
            "set_status_vital_failed",
            K8sInstanceMock::set_status_vital_failed,
        )
        .register_fn("set_status_scalables", K8sInstanceMock::set_status_scalables)
        .register_fn(
            "set_status_scalable_failed",
            K8sInstanceMock::set_status_scalable_failed,
        )
        .register_fn("set_status_others", K8sInstanceMock::set_status_others)
        .register_fn(
            "set_status_other_failed",
            K8sInstanceMock::set_status_other_failed,
        )
        .register_fn("set_status_posts", K8sInstanceMock::set_status_posts)
        .register_fn("set_status_post_failed", K8sInstanceMock::set_status_post_failed)
        .register_fn("set_status_init_failed", K8sInstanceMock::set_status_init_failed)
        .register_fn(
            "set_status_schedule_backup_failed",
            K8sInstanceMock::set_status_schedule_backup_failed,
        );
}

pub fn k8smock_rhai_register(engine: &mut Engine, mocks: Vec<Dynamic>, created: Arc<Mutex<Vec<Dynamic>>>) {
    let arced_mocks = Arc::new(Mutex::new(mocks));
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
    engine
        .register_type_with_name::<K8sObjectMock>("K8sObject")
        .register_get("kind", K8sObjectMock::get_kind)
        .register_get("metadata", K8sObjectMock::get_metadata)
        .register_fn("delete", K8sObjectMock::rhai_delete)
        .register_fn("wait_condition", K8sObjectMock::wait_condition)
        .register_fn("wait_status", K8sObjectMock::wait_status)
        .register_fn("wait_status_prop", K8sObjectMock::wait_status_prop)
        .register_fn("wait_status_string", K8sObjectMock::wait_status_string)
        .register_fn("wait_deleted", K8sObjectMock::rhai_wait_deleted);
    engine
        .register_type_with_name::<K8sGenericMock>("K8sGeneric")
        .register_fn("k8s_resource", new_global)
        .register_fn("k8s_resource", new_ns)
        .register_fn("k8s_resource", new_group_ns)
        .register_fn("list", K8sGenericMock::rhai_list)
        .register_fn("list", K8sGenericMock::rhai_list_labels)
        .register_fn("update_k8s_crd_cache", update_cache)
        .register_fn("list_meta", K8sGenericMock::rhai_list_meta)
        .register_fn("get", K8sGenericMock::rhai_get)
        .register_fn("get_meta", K8sGenericMock::rhai_get_meta)
        .register_fn("get_obj", K8sGenericMock::rhai_get_obj)
        .register_fn("delete", K8sGenericMock::rhai_delete)
        .register_fn("create", K8sGenericMock::rhai_create)
        .register_fn("replace", K8sGenericMock::rhai_replace)
        .register_fn("patch", K8sGenericMock::rhai_patch)
        .register_fn("apply", K8sGenericMock::rhai_apply)
        .register_fn("exist", K8sGenericMock::rhai_exist)
        .register_get("scope", K8sGenericMock::rhai_get_scope);

    // ── K8sRaw mock ─────────────────────────────────────────────────────
    engine
        .register_type_with_name::<K8sRawMock>("K8sRaw")
        .register_fn("new_k8s_raw", K8sRawMock::new)
        .register_fn("get_url", K8sRawMock::rhai_get_url)
        .register_fn("get_api_resources", K8sRawMock::rhai_get_api_resources)
        .register_fn("get_cluster_version", K8sRawMock::rhai_get_api_version);

    // ── K8sWorkload mocks ───────────────────────────────────────────────
    // All four workload types share K8sWorkloadMock which extracts
    // metadata/spec/status from the Dynamic mock object.

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

    // ── Instance mocks ──────────────────────────────────────────────────

    // ServiceInstance
    let inst_mocks = arced_mocks.clone();
    let get_svc = move |ns: String, name: String| -> RhaiRes<K8sInstanceMock> {
        let mock: Arc<Mutex<Vec<Dynamic>>> = inst_mocks.clone();
        find_instance_mock(mock, "ServiceInstance", &ns, &name)
    };
    let inst_mocks = arced_mocks.clone();
    let list_svc = move |ns: String| -> RhaiRes<Dynamic> {
        let mock: Arc<Mutex<Vec<Dynamic>>> = inst_mocks.clone();
        list_instance_mocks(mock, "ServiceInstance", &ns)
    };
    engine
        .register_type_with_name::<K8sInstanceMock>("ServiceInstance")
        .register_fn("get_service_instance", get_svc)
        .register_fn("list_service_instance", list_svc)
        .register_fn("list_services_names", || -> RhaiRes<Vec<String>> { Ok(vec![]) })
        .register_fn("set_status_crds", K8sInstanceMock::set_status_crds)
        .register_fn("set_status_crd_failed", K8sInstanceMock::set_status_crd_failed);
    register_instance_common(engine);
    register_instance_children(engine);

    // SystemInstance
    let inst_mocks = arced_mocks.clone();
    let get_sys = move |ns: String, name: String| -> RhaiRes<K8sInstanceMock> {
        let mock: Arc<Mutex<Vec<Dynamic>>> = inst_mocks.clone();
        find_instance_mock(mock, "SystemInstance", &ns, &name)
    };
    let inst_mocks = arced_mocks.clone();
    let list_sys = move |ns: String| -> RhaiRes<Dynamic> {
        let mock: Arc<Mutex<Vec<Dynamic>>> = inst_mocks.clone();
        list_instance_mocks(mock, "SystemInstance", &ns)
    };
    engine
        .register_type_with_name::<K8sInstanceMock>("SystemInstance")
        .register_fn("get_system_instance", get_sys)
        .register_fn("list_system_instance", list_sys)
        .register_fn("set_status_crds", K8sInstanceMock::set_status_crds)
        .register_fn("set_status_crd_failed", K8sInstanceMock::set_status_crd_failed)
        .register_fn("set_status_systems", K8sInstanceMock::set_status_systems)
        .register_fn(
            "set_status_system_failed",
            K8sInstanceMock::set_status_system_failed,
        );
    register_instance_common(engine);

    // TenantInstance
    let inst_mocks = arced_mocks.clone();
    let get_tnt = move |ns: String, name: String| -> RhaiRes<K8sInstanceMock> {
        let mock: Arc<Mutex<Vec<Dynamic>>> = inst_mocks.clone();
        find_instance_mock(mock, "TenantInstance", &ns, &name)
    };
    let inst_mocks = arced_mocks.clone();
    let list_tnt = move |ns: String| -> RhaiRes<Dynamic> {
        let mock: Arc<Mutex<Vec<Dynamic>>> = inst_mocks.clone();
        list_instance_mocks(mock, "TenantInstance", &ns)
    };
    engine
        .register_type_with_name::<K8sInstanceMock>("TenantInstance")
        .register_fn("get_tenant_instance", get_tnt)
        .register_fn("list_tenant_instance", list_tnt)
        .register_fn("get_tenant_name", K8sInstanceMock::get_tenant_name)
        .register_fn("get_tenant_namespaces", K8sInstanceMock::get_tenant_namespaces)
        .register_fn(
            "get_tenant_services_names",
            K8sInstanceMock::get_tenant_services_names,
        );
    register_instance_common(engine);
    register_instance_children(engine);

    // JukeBox (cluster-scoped)
    let jb_mocks = arced_mocks.clone();
    let get_jb = move |name: String| -> RhaiRes<K8sJukeBoxMock> {
        let mock: Arc<Mutex<Vec<Dynamic>>> = jb_mocks.clone();
        find_jukebox_mock(mock, &name)
    };
    let jb_mocks = arced_mocks;
    let list_jb = move || -> RhaiRes<Dynamic> {
        let mock: Arc<Mutex<Vec<Dynamic>>> = jb_mocks.clone();
        list_jukebox_mocks(mock)
    };
    engine
        .register_type_with_name::<K8sJukeBoxMock>("JukeBox")
        .register_fn("get_jukebox", get_jb)
        .register_fn("list_jukebox", list_jb)
        .register_fn("set_status_updated", K8sJukeBoxMock::set_status_updated)
        .register_fn("set_status_failed", K8sJukeBoxMock::set_status_failed)
        .register_get("metadata", K8sJukeBoxMock::get_metadata)
        .register_get("spec", K8sJukeBoxMock::get_spec)
        .register_get("status", K8sJukeBoxMock::get_status);
}
