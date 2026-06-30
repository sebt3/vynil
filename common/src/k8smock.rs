use crate::RhaiRes;
use rhai::{Dynamic, Engine, Map, serde::to_dynamic};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

// Re-export for backward compatibility (agent tests import from common::k8smock)
pub use vynil_core::oci_mock::oci_mock_rhai_register;

// ── K8sInstance mock (ServiceInstance, SystemInstance, TenantInstance) ────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct K8sInstanceMockObj {
    pub obj: Dynamic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct K8sInstanceMock {
    pub obj: Dynamic,
    pub mocks: Arc<Mutex<Vec<Dynamic>>>,
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
        self.persist();
    }

    fn persist(&self) {
        if !self.obj.is_map() {
            return;
        }
        let map = self.obj.as_map_ref().unwrap();
        let kind = match map.get("kind").and_then(|k| k.clone().into_string().ok()) {
            Some(k) => k,
            None => return,
        };
        let meta: Map = match map.get("metadata") {
            Some(m) if m.is_map() => m.as_map_ref().unwrap().clone(),
            _ => return,
        };
        let name = match meta.get("name").and_then(|n| n.clone().into_string().ok()) {
            Some(n) => n,
            None => return,
        };
        let ns = match meta.get("namespace").and_then(|n| n.clone().into_string().ok()) {
            Some(n) => n,
            None => return,
        };
        let mut mocks = self.mocks.lock().unwrap();
        for entry in mocks.iter_mut() {
            if !entry.is_map() {
                continue;
            }
            let entry_map: Map = entry.as_map_ref().unwrap().clone();
            let entry_kind = entry_map.get("kind").and_then(|k| k.clone().into_string().ok());
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
            if entry_kind.as_deref() == Some(kind.as_str())
                && entry_name.as_deref() == Some(name.as_str())
                && entry_ns.as_deref() == Some(ns.as_str())
            {
                *entry = self.obj.clone();
                return;
            }
        }
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
        if let Ok(m) = status.as_map_ref()
            && let Some(v) = m.get("tfstate")
            && let Ok(s) = v.clone().into_string()
        {
            return Ok(s);
        }
        Ok(String::new())
    }

    pub fn get_rhaistate(&mut self) -> RhaiRes<String> {
        let status = self.get_sub("status")?;
        if let Ok(m) = status.as_map_ref()
            && let Some(v) = m.get("rhaistate")
            && let Ok(s) = v.clone().into_string()
        {
            return Ok(s);
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

    pub fn set_missing_init_version(&mut self, _version: String) -> RhaiRes<Self> {
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
        if let Ok(status) = self.get_sub("status")
            && let Ok(status_map) = status.as_map_ref()
            && let Some(services) = status_map.get("services")
            && let Ok(arr) = services.clone().into_array()
        {
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
        String::new()
    }

    // ── Tenant-specific ─────────────────────────────────────────────────

    pub fn get_tenant_name(&mut self) -> RhaiRes<String> {
        if let Ok(meta) = self.get_sub("metadata")
            && let Ok(m) = meta.as_map_ref()
            && let Some(ns) = m.get("namespace")
            && let Ok(s) = ns.clone().into_string()
        {
            for m in self.mocks.lock().unwrap().clone() {
                if !m.is_map() {
                    continue;
                }
                let map = m.as_map_ref().unwrap();
                if !map.contains_key("kind") || !map["kind"].is_string() {
                    continue;
                }
                if map["kind"].clone().into_string().unwrap() != "Namespace" {
                    continue;
                }
                if !map.contains_key("metadata") || !map["metadata"].is_map() {
                    continue;
                }
                let meta = map["metadata"].as_map_ref().unwrap();
                let name_match = meta.contains_key("name")
                    && meta["name"].is_string()
                    && meta["name"].clone().into_string().unwrap() == s;
                if name_match && meta.contains_key("labels") && meta["labels"].is_map() {
                    let labels = meta["labels"].as_map_ref().unwrap();
                    let label_key = std::env::var("TENANT_LABEL")
                        .unwrap_or_else(|_| "vynil.solidite.fr/tenant".to_string());
                    if labels.clone().keys().any(|k| k == &label_key) {
                        let tenant = labels[label_key.as_str()].clone();
                        return Ok(tenant.to_string());
                    }
                }
            }
            return Ok(s);
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
            return Ok(K8sInstanceMock {
                obj: m.clone(),
                mocks: mocks.clone(),
            });
        }
    }
    Err(format!("Failed to find {kind} {name} in namespace {namespace} in the Mock database").into())
}

fn list_instance_mocks(mocks: Arc<Mutex<Vec<Dynamic>>>, kind: &str, namespace: &str) -> RhaiRes<Dynamic> {
    let items: Vec<K8sInstanceMockObj> = mocks
        .lock()
        .unwrap()
        .clone()
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
        .map(|m| K8sInstanceMockObj { obj: m.clone() })
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

    pub fn set_status_packages_merge(&mut self, _filter: String, packages: Dynamic) -> RhaiRes<Self> {
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
    let items: Vec<K8sJukeBoxMock> = mocks
        .lock()
        .unwrap()
        .clone()
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
        .register_fn(
            "set_missing_init_version",
            K8sInstanceMock::set_missing_init_version,
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

    // Register generic K8s mocks from core
    vynil_core::k8s_mock::k8s_mock_rhai_register(engine, arced_mocks.clone(), created.clone());

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
        .register_fn(
            "set_status_packages_merge",
            K8sJukeBoxMock::set_status_packages_merge,
        )
        .register_fn("set_status_failed", K8sJukeBoxMock::set_status_failed)
        .register_get("metadata", K8sJukeBoxMock::get_metadata)
        .register_get("spec", K8sJukeBoxMock::get_spec)
        .register_get("status", K8sJukeBoxMock::get_status);
}
