use crate::{Result, RhaiRes};
use kube::{api::DynamicObject, runtime::wait::Condition};
use rhai::{Dynamic, Engine, serde::to_dynamic};

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
        if self.obj.is_map() &&
                self.obj.as_map_ref().unwrap().contains_key("metadata") &&
                self.obj.as_map_ref().unwrap()["metadata"].is_map() {
            Ok(self.obj.as_map_ref().unwrap()["metadata"].clone())
        } else {
            Err(format!("Failed to extract metadata from a {}", self.kind).into())
        }
    }

    pub fn get_kind(&mut self) -> String {
        self.kind.clone()
    }

    pub fn is_condition(_cond: String) -> impl Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| {
            true
        }
    }

    pub fn wait_condition(&mut self, _condition: String, _timeout: i64) -> RhaiRes<()> {
        Ok(())
    }

    pub fn is_status(_prop: String) -> impl Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| {
            true
        }
    }

    pub fn have_status(_prop: String) -> impl Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| {
            true
        }
    }

    pub fn have_status_value(_prop: String, _value: String) -> impl Condition<DynamicObject> {
        move |_obj: Option<&DynamicObject>| {
            true
        }
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
        move |_obj: Option<&DynamicObject>| {
            true
        }
    }

    pub fn wait_for(
        &mut self,
        _condition: Box<dyn Fn(&DynamicObject) -> Result<bool, Box<rhai::EvalAltResult>>>,
        _timeout: i64,
    ) -> RhaiRes<()> {
        Ok(())
    }
}


#[derive(Clone, Debug)]
pub struct K8sGenericMock {
    pub kind: String,
    pub ns: Option<String>,
    pub mocks: Vec<Dynamic>
}

impl K8sGenericMock {
    #[must_use]
    pub fn new(mocks: Vec<Dynamic>, name: &str, ns: Option<String>) -> Self {
        let mocks: Vec<Dynamic> = mocks.into_iter()
            .filter(|m|
                m.is_map() &&
                m.as_map_ref().unwrap().contains_key("kind") &&
                m.as_map_ref().unwrap()["kind"].is_string() &&
                m.as_map_ref().unwrap()["kind"].clone().into_string().unwrap() == name
            ).collect();
        if ns.is_some() {
            Self {
                kind: name.into(),
                ns: ns.clone(),
                mocks: mocks.into_iter()
                    .filter(|m|
                        m.is_map() &&
                        m.as_map_ref().unwrap().contains_key("metadata") &&
                        m.as_map_ref().unwrap()["metadata"].is_map() &&
                        m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap().contains_key("namespace") &&
                        m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["namespace"].is_string() &&
                        m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["namespace"].clone().into_string().unwrap() == ns.clone().unwrap()
                    ).collect(),
            }
        } else {
            Self {
                kind: name.into(),
                ns,
                mocks,
            }
        }
    }

    #[must_use]
    pub fn new_api_version(mocks: Vec<Dynamic>, _api_group: &str, _version: &str, name: &str, ns: Option<String>) -> Self {
        Self::new(mocks, name, ns)
    }

    pub fn new_ns(mocks: Vec<Dynamic>, name: String, ns: String) -> Self {
        Self::new(mocks, name.as_str(), Some(ns))
    }

    pub fn new_global(mocks: Vec<Dynamic>, name: String) -> Self {
        Self::new(mocks, name.as_str(), None)
    }

    pub fn new_group_ns(mocks: Vec<Dynamic>, api_version: String, name: String, ns: String) -> Self {
        let arr = api_version.split("/").collect::<Vec<&str>>();
        if arr.len() > 1 {
            Self::new_api_version(mocks, arr[0], arr[1], name.as_str(), Some(ns))
        } else {
            Self::new(mocks, name.as_str(), Some(ns))
        }
    }

    pub fn rhai_get_scope(&mut self) -> String {
        "namespace".to_string()
    }

    pub fn rhai_exist(&mut self) -> RhaiRes<Dynamic> {
        Ok(true.into())
    }

    pub fn rhai_list(&mut self) -> RhaiRes<Dynamic> {
        to_dynamic(self.mocks.clone())
    }

    pub fn rhai_list_labels(&mut self, _labels: String) -> RhaiRes<Dynamic> {
        //TODO : to the label filtering
        to_dynamic(self.mocks.clone())
    }

    pub fn rhai_list_meta(&mut self) -> RhaiRes<Dynamic> {
        self.rhai_list()
    }

    pub fn rhai_get(&mut self, name: String) -> RhaiRes<Dynamic> {
        let found: Vec<Dynamic> = self.mocks.clone().into_iter()
            .filter(|m|
                m.is_map() &&
                m.as_map_ref().unwrap().contains_key("metadata") &&
                m.as_map_ref().unwrap()["metadata"].is_map() &&
                m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap().contains_key("name") &&
                m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["name"].is_string() &&
                m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["name"].clone().into_string().unwrap() == name
            ).collect();
        if found.len() > 0 {
            Ok(found[0].clone())
        } else {
            Err(format!("Failed to find {} {name} in the Mock database", self.kind).into())
        }
    }

    pub fn rhai_get_meta(&mut self, name: String) -> RhaiRes<Dynamic> {self.rhai_get(name)}

    pub fn rhai_get_obj(&mut self, name: String) -> RhaiRes<K8sObjectMock> {
        let found: Vec<Dynamic> = self.mocks.clone().into_iter()
            .filter(|m|
                m.is_map() &&
                m.as_map_ref().unwrap().contains_key("metadata") &&
                m.as_map_ref().unwrap()["metadata"].is_map() &&
                m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap().contains_key("name") &&
                m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["name"].is_string() &&
                m.as_map_ref().unwrap()["metadata"].as_map_ref().unwrap()["name"].clone().into_string().unwrap() == name
            ).collect();
        if found.len() > 0 {
            Ok(K8sObjectMock {
                obj: found[0].clone(),
                kind: self.kind.clone(),
            })
        } else {
            Err(format!("Failed to find {} {name} in the Mock database", self.kind).into())
        }
    }

    pub fn rhai_delete(&mut self, _name: String) -> RhaiRes<()> {Ok(())}
    pub fn rhai_replace(&mut self, _name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {Ok(data)}
    pub fn rhai_patch(&mut self, _name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {Ok(data)}

    // To be collected for asserts
    pub fn rhai_create(&mut self, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        Ok(data)
    }
    pub fn rhai_apply(&mut self, _name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        Ok(data)
    }
}

pub fn k8smock_rhai_register(engine: &mut Engine, mocks: Vec<Dynamic>) {
    let lmocks = mocks.clone();
    let new_global = move |name: String| -> K8sGenericMock {
        let mock = lmocks.clone();
        K8sGenericMock::new_global(mock.clone(), name)
    };
    let lmocks = mocks.clone();
    let new_ns = move |name: String, ns: String| -> K8sGenericMock {
        let mock = lmocks.clone();
        K8sGenericMock::new_ns(mock, name, ns)
    };
    let lmocks = mocks.clone();
    let new_group_ns = move |apiv: String, name: String, ns: String| -> K8sGenericMock {
        let mock = lmocks.clone();
        K8sGenericMock::new_group_ns(mock, apiv, name, ns)
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
}
