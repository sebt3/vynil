use tokio::sync::RwLock;

use crate::{
    Error, Result, RhaiRes,
    context::{get_client, get_client_name, get_labels, get_owner, get_owner_ns},
    rhai_err,
};
use kube::{
    Client, ResourceExt,
    api::{
        Api, DeleteParams, DynamicObject, ListParams, ObjectList, PartialObjectMeta, Patch, PatchParams,
        PostParams,
    },
    discovery::{ApiCapabilities, ApiResource, Discovery, Scope},
    runtime::wait::{Condition, await_condition, conditions},
};
use rhai::{Dynamic, Engine, serde::to_dynamic};
use serde_json::json;

type DynObjCondition = Box<dyn Fn(&DynamicObject) -> Result<bool, Box<rhai::EvalAltResult>>>;

lazy_static::lazy_static! {
    pub static ref CLIENT: Client = get_client();
}
async fn excluded_apiservice_groups() -> Vec<String> {
    let ar = ApiResource {
        group: "apiregistration.k8s.io".to_string(),
        version: "v1".to_string(),
        api_version: "apiregistration.k8s.io/v1".to_string(),
        kind: "APIService".to_string(),
        plural: "apiservices".to_string(),
    };
    let api: Api<DynamicObject> = Api::all_with(CLIENT.clone(), &ar);
    match api.list(&ListParams::default()).await {
        Ok(list) => list
            .items
            .iter()
            .filter_map(|obj| {
                obj.data
                    .get("spec")
                    .and_then(|s| s.get("group"))
                    .and_then(|g| g.as_str())
                    .filter(|g| !g.is_empty())
                    .map(|g| g.to_string())
            })
            .collect(),
        Err(e) => {
            tracing::warn!("E_DISCOVERY_WARN: cannot list APIServices ({e}), proceeding without exclusions");
            vec![]
        }
    }
}
async fn async_populate_cache() -> Discovery {
    let excluded = excluded_apiservice_groups().await;
    let excluded_refs: Vec<&str> = excluded.iter().map(|s| s.as_str()).collect();
    Discovery::new(CLIENT.clone())
        .exclude(&excluded_refs)
        .run()
        .await
        .expect("create discovery (excluding api-services)")
}
fn populate_cache() -> Discovery {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async move {
            let excluded = excluded_apiservice_groups().await;
            let excluded_refs: Vec<&str> = excluded.iter().map(|s| s.as_str()).collect();
            Discovery::new(CLIENT.clone())
                .exclude(&excluded_refs)
                .run()
                .await
                .expect("create discovery (excluding api-services)")
        })
    })
}
lazy_static::lazy_static! {
    pub static ref CACHE: RwLock<Discovery> = RwLock::new(populate_cache());
}

pub fn update_cache() {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async move {
            match tokio::time::timeout(std::time::Duration::from_secs(60), async_populate_cache()).await {
                Ok(discovery) => {
                    *CACHE.write().await = discovery;
                }
                Err(_) => {
                    tracing::warn!(
                        "E_DISCOVERY_TIMEOUT: update_k8s_crd_cache exceeded 30s, keeping old cache"
                    );
                }
            }
        })
    })
}

#[derive(Clone, Debug)]
pub struct K8sObject {
    pub api: Api<DynamicObject>,
    pub obj: PartialObjectMeta,
    pub kind: String,
}
impl K8sObject {
    pub fn rhai_delete(&mut self) -> RhaiRes<()> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                self.api
                    .delete(&self.obj.name_any(), &DeleteParams::foreground())
                    .await
                    .map_err(Error::KubeError)
                    .map(|_| ())
            })
        })
        .map_err(rhai_err)
    }

    pub fn rhai_wait_deleted(&mut self, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        let uid = self.obj.uid().unwrap();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let cond = await_condition(self.api.clone(), &name, conditions::is_deleted(&uid));
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(Error::Elapsed)
            })
        })
        .map_err(rhai_err)?
        .map_err(|e| rhai_err(Error::KubeWaitError(e)))
        .map(|_| ())
    }

    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_value(self.obj.metadata.clone())
            .map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }

    pub fn get_kind(&mut self) -> String {
        if let Some(t) = self.obj.types.clone() {
            t.kind
        } else {
            "".to_string()
        }
    }

    pub fn original_kind(&mut self) -> String {
        self.kind.clone()
    }

    pub fn is_condition(cond: String) -> impl Condition<DynamicObject> {
        move |obj: Option<&DynamicObject>| {
            if let Some(dynobj) = &obj
                && dynobj.data.is_object()
                && dynobj
                    .data
                    .as_object()
                    .unwrap()
                    .keys()
                    .collect::<Vec<&String>>()
                    .contains(&&"status".to_string())
            {
                let status = dynobj.data.as_object().unwrap()["status"].clone();
                if status.is_object()
                    && status
                        .as_object()
                        .unwrap()
                        .keys()
                        .collect::<Vec<&String>>()
                        .contains(&&"conditions".to_string())
                {
                    let conditions = status.as_object().unwrap()["conditions"].clone();
                    if conditions.is_array()
                        && conditions.as_array().unwrap().iter().any(|c| {
                            c.is_object()
                                && c.as_object()
                                    .unwrap()
                                    .keys()
                                    .collect::<Vec<&String>>()
                                    .contains(&&"type".to_string())
                                && c.as_object().unwrap()["type"].is_string()
                                && c.as_object().unwrap()["type"].as_str().unwrap() == cond
                                && c.as_object()
                                    .unwrap()
                                    .keys()
                                    .collect::<Vec<&String>>()
                                    .contains(&&"status".to_string())
                                && c.as_object().unwrap()["status"].is_string()
                                && c.as_object().unwrap()["status"].as_str().unwrap() == "True"
                        })
                    {
                        return true;
                    }
                }
            }
            false
        }
    }

    pub fn wait_condition(&mut self, condition: String, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        let cond = await_condition(self.api.clone(), &name, Self::is_condition(condition));
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(Error::Elapsed)
            })
        })
        .map_err(rhai_err)?
        .map_err(Error::KubeWaitError)
        .map_err(rhai_err)?;
        Ok(())
    }

    pub fn is_status(prop: String) -> impl Condition<DynamicObject> {
        move |obj: Option<&DynamicObject>| {
            if let Some(dynobj) = &obj
                && dynobj.data.is_object()
                && dynobj
                    .data
                    .as_object()
                    .unwrap()
                    .keys()
                    .collect::<Vec<&String>>()
                    .contains(&&"status".to_string())
            {
                let status = dynobj.data.as_object().unwrap()["status"].clone();
                if status.is_object()
                    && status
                        .as_object()
                        .unwrap()
                        .keys()
                        .collect::<Vec<&String>>()
                        .contains(&&prop)
                {
                    let conditions = status.as_object().unwrap()[&prop].clone();
                    if conditions.is_boolean() && conditions.as_bool().unwrap() {
                        return true;
                    }
                }
            }
            false
        }
    }

    pub fn have_status(prop: String) -> impl Condition<DynamicObject> {
        move |obj: Option<&DynamicObject>| {
            if let Some(dynobj) = &obj
                && dynobj.data.is_object()
                && dynobj
                    .data
                    .as_object()
                    .unwrap()
                    .keys()
                    .collect::<Vec<&String>>()
                    .contains(&&"status".to_string())
            {
                let status = dynobj.data.as_object().unwrap()["status"].clone();
                if status.is_object()
                    && status
                        .as_object()
                        .unwrap()
                        .keys()
                        .collect::<Vec<&String>>()
                        .contains(&&prop)
                {
                    let conditions = status.as_object().unwrap()[&prop].clone();
                    if !conditions.is_null() {
                        return true;
                    }
                }
            }
            false
        }
    }

    pub fn have_status_value(prop: String, value: String) -> impl Condition<DynamicObject> {
        move |obj: Option<&DynamicObject>| {
            if let Some(dynobj) = &obj
                && dynobj.data.is_object()
                && dynobj
                    .data
                    .as_object()
                    .unwrap()
                    .keys()
                    .collect::<Vec<&String>>()
                    .contains(&&"status".to_string())
            {
                let status = dynobj.data.as_object().unwrap()["status"].clone();
                if status.is_object()
                    && status
                        .as_object()
                        .unwrap()
                        .keys()
                        .collect::<Vec<&String>>()
                        .contains(&&prop)
                {
                    let conditions = status.as_object().unwrap()[&prop].clone();
                    if conditions.is_string() {
                        return conditions.as_str().unwrap() == value;
                    }
                }
            }
            false
        }
    }

    pub fn wait_status(&mut self, prop: String, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        tracing::debug!("wait_status({}) for {} {}", &prop, self.kind, name);
        let cond = await_condition(self.api.clone(), &name, Self::is_status(prop));
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(Error::Elapsed)
            })
        })
        .map_err(rhai_err)?
        .map_err(Error::KubeWaitError)
        .map_err(rhai_err)?;
        Ok(())
    }

    pub fn wait_status_prop(&mut self, prop: String, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        tracing::debug!("wait_status({}) for {} {}", &prop, self.kind, name);
        let cond = await_condition(self.api.clone(), &name, Self::have_status(prop));
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(Error::Elapsed)
            })
        })
        .map_err(rhai_err)?
        .map_err(Error::KubeWaitError)
        .map_err(rhai_err)?;
        Ok(())
    }

    pub fn wait_status_string(&mut self, prop: String, value: String, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        tracing::debug!("wait_status({}) for {} {}", &prop, self.kind, name);
        let cond = await_condition(self.api.clone(), &name, Self::have_status_value(prop, value));
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(Error::Elapsed)
            })
        })
        .map_err(rhai_err)?
        .map_err(Error::KubeWaitError)
        .map_err(rhai_err)?;
        Ok(())
    }

    pub fn is_for(cond: DynObjCondition) -> impl Condition<DynamicObject> {
        move |obj: Option<&DynamicObject>| {
            if let Some(dynobj) = &obj
                && dynobj.data.is_object()
            {
                return cond(dynobj).unwrap_or_else(|e| {
                    tracing::warn!("wait_for closure error: {:?}", e);
                    false
                });
            }
            false
        }
    }

    pub fn wait_for(&mut self, condition: DynObjCondition, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        let cond = await_condition(self.api.clone(), &name, Self::is_for(condition));
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(Error::Elapsed)
            })
        })
        .map_err(rhai_err)?
        .map_err(Error::KubeWaitError)
        .map_err(rhai_err)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct K8sGeneric {
    pub api: Option<Api<DynamicObject>>,
    pub ns: Option<String>,
    pub scope: Scope,
    pub kind: String,
}

// TODO: scale et exec

impl K8sGeneric {
    #[must_use]
    pub fn new(name: &str, ns: Option<String>) -> K8sGeneric {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                if let Some((res, cap)) = CACHE
                    .read()
                    .await
                    .groups()
                    .flat_map(|group| {
                        group
                            .resources_by_stability()
                            .into_iter()
                            .map(move |res: (ApiResource, ApiCapabilities)| (group, res))
                    })
                    .filter(|(_, (res, _))| {
                        name.eq_ignore_ascii_case(&res.kind) || name.eq_ignore_ascii_case(&res.plural)
                    })
                    .min_by_key(|(group, _res)| group.name())
                    .map(|(_, res)| res)
                {
                    tracing::debug!("K8sGeneric::new Using {}/{}/{}", res.group, res.version, res.kind);
                    let api = if cap.scope == Scope::Cluster || ns.is_none() {
                        Api::all_with(CLIENT.clone(), &res)
                    } else if let Some(namespace) = ns.clone() {
                        Api::namespaced_with(CLIENT.clone(), &namespace, &res)
                    } else {
                        Api::default_namespaced_with(CLIENT.clone(), &res)
                    };
                    K8sGeneric {
                        api: Some(api),
                        ns,
                        scope: cap.scope,
                        kind: res.kind,
                    }
                } else {
                    K8sGeneric {
                        api: None,
                        ns: None,
                        scope: Scope::Cluster,
                        kind: String::new(),
                    }
                }
            })
        })
    }

    #[must_use]
    pub fn new_api_version(api_group: &str, version: &str, name: &str, ns: Option<String>) -> K8sGeneric {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                if let Some((res, cap)) = CACHE
                    .read()
                    .await
                    .groups()
                    .flat_map(|group| {
                        group
                            .resources_by_stability()
                            .into_iter()
                            .map(move |res: (ApiResource, ApiCapabilities)| (group, res))
                    })
                    .filter(|(group, (res, _))| {
                        group.name() == api_group
                            && res.version == version
                            && (name.eq_ignore_ascii_case(&res.kind)
                                || name.eq_ignore_ascii_case(&res.plural))
                    })
                    .min_by_key(|(group, _res)| group.name())
                    .map(|(_, res)| res)
                {
                    tracing::debug!(
                        "K8sGeneric::new_api_version Using {}/{}/{}",
                        res.group,
                        res.version,
                        res.kind
                    );
                    let api = if cap.scope == Scope::Cluster || ns.is_none() {
                        Api::all_with(CLIENT.clone(), &res)
                    } else if let Some(namespace) = ns.clone() {
                        Api::namespaced_with(CLIENT.clone(), &namespace, &res)
                    } else {
                        Api::default_namespaced_with(CLIENT.clone(), &res)
                    };
                    K8sGeneric {
                        api: Some(api),
                        ns,
                        scope: cap.scope,
                        kind: res.kind,
                    }
                } else {
                    K8sGeneric {
                        api: None,
                        ns: None,
                        scope: Scope::Cluster,
                        kind: String::new(),
                    }
                }
            })
        })
    }

    pub fn new_ns(name: String, ns: String) -> K8sGeneric {
        K8sGeneric::new(name.as_str(), Some(ns))
    }

    pub fn new_global(name: String) -> K8sGeneric {
        K8sGeneric::new(name.as_str(), None)
    }

    pub fn new_group_ns(api_version: String, name: String, ns: String) -> K8sGeneric {
        let arr = api_version.split("/").collect::<Vec<&str>>();
        if arr.len() > 1 {
            K8sGeneric::new_api_version(arr[0], arr[1], name.as_str(), Some(ns))
        } else {
            K8sGeneric::new(name.as_str(), Some(ns))
        }
    }

    pub fn rhai_get_scope(&mut self) -> String {
        if self.scope == Scope::Cluster {
            "cluster".to_string()
        } else {
            "namespace".to_string()
        }
    }

    pub fn exist(&self) -> bool {
        self.api.is_some()
    }

    pub fn rhai_exist(&mut self) -> RhaiRes<Dynamic> {
        to_dynamic(self.api.is_some())
    }

    pub fn list(&self) -> Result<ObjectList<DynamicObject>> {
        if let Some(api) = self.api.clone() {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async move { api.list(&ListParams::default()).await.map_err(Error::KubeError) })
            })
        } else {
            Err(Error::UnsupportedMethod)
        }
    }

    pub fn rhai_list(&mut self) -> RhaiRes<Dynamic> {
        let res = self.list().map_err(rhai_err)?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }

    pub fn list_labels(&self, labels: String) -> Result<ObjectList<DynamicObject>> {
        if let Some(api) = self.api.clone() {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    let mut lp = ListParams::default();
                    lp = lp.labels(&labels);
                    api.list(&lp).await.map_err(Error::KubeError)
                })
            })
        } else {
            Err(Error::UnsupportedMethod)
        }
    }

    pub fn rhai_list_labels(&mut self, labels: String) -> RhaiRes<Dynamic> {
        let res = self.list_labels(labels).map_err(rhai_err)?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }

    pub fn list_meta(&self) -> Result<ObjectList<PartialObjectMeta>> {
        if let Some(api) = self.api.clone() {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    api.list_metadata(&ListParams::default())
                        .await
                        .map_err(Error::KubeError)
                })
            })
        } else {
            Err(Error::UnsupportedMethod)
        }
    }

    pub fn rhai_list_meta(&mut self) -> RhaiRes<Dynamic> {
        let res = self.list_meta().map_err(rhai_err)?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }

    pub fn get(&self, name: &str) -> Result<DynamicObject> {
        if let Some(api) = self.api.clone() {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async move { api.get(name).await.map_err(Error::KubeError) })
            })
        } else {
            Err(Error::UnsupportedMethod)
        }
    }

    pub fn rhai_get(&mut self, name: String) -> RhaiRes<Dynamic> {
        let res = self.get(&name).map_err(rhai_err)?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }

    pub fn get_meta(&self, name: &str) -> Result<PartialObjectMeta> {
        if let Some(api) = self.api.clone() {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async move { api.get_metadata(name).await.map_err(Error::KubeError) })
            })
        } else {
            Err(Error::UnsupportedMethod)
        }
    }

    pub fn rhai_get_meta(&mut self, name: String) -> RhaiRes<Dynamic> {
        let res = self.get_meta(&name).map_err(rhai_err)?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }

    pub fn rhai_get_obj(&mut self, name: String) -> RhaiRes<K8sObject> {
        let res = self.get_meta(&name).map_err(rhai_err)?;
        Ok(K8sObject {
            api: self.api.clone().unwrap(),
            obj: res,
            kind: self.kind.clone(),
        })
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        if let Some(api) = self.api.clone() {
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    api.delete(name, &DeleteParams::foreground())
                        .await
                        .map_err(Error::KubeError)
                        .map(|_| ())
                })
            })
        } else {
            Err(Error::UnsupportedMethod)
        }
    }

    pub fn rhai_delete(&mut self, name: String) -> RhaiRes<()> {
        self.delete(&name).map_err(rhai_err)
    }

    pub fn create(&self, data: serde_json::Map<String, serde_json::Value>) -> Result<DynamicObject> {
        if let Some(api) = self.api.clone() {
            let mut handle = data.clone();
            if let Some(labels) = get_labels() {
                if !handle["metadata"].as_object().unwrap().contains_key("labels") {
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("labels".to_string(), json!({}));
                } else if !handle["metadata"].as_object_mut().unwrap()["labels"].is_object() {
                    handle["metadata"].as_object_mut().unwrap().remove_entry("labels");
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("labels".to_string(), json!({}));
                }
                for (k, v) in labels.as_object().unwrap() {
                    if !handle["metadata"].as_object_mut().unwrap()["labels"]
                        .as_object_mut()
                        .unwrap()
                        .keys()
                        .any(|name| name == k)
                    {
                        handle["metadata"].as_object_mut().unwrap()["labels"]
                            .as_object_mut()
                            .unwrap()
                            .insert(k.to_string(), v.clone());
                    }
                }
            }
            if self.scope == Scope::Namespaced
                && let Some(owner) = get_owner()
                && let Some(ns) = get_owner_ns()
                && let Some(mine) = self.ns.clone()
                && ns == mine
            {
                if handle["metadata"]
                    .as_object()
                    .unwrap()
                    .contains_key("ownerReferences")
                {
                    handle["metadata"].as_object_mut().unwrap()["ownerReferences"]
                        .as_array_mut()
                        .unwrap()
                        .push(owner);
                } else {
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("ownerReferences".to_string(), vec![owner].into());
                }
            }
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    match serde_json::from_value(handle.into()) {
                        Ok(obj) => api
                            .create(&PostParams::default(), &obj)
                            .await
                            .map_err(Error::KubeError),
                        Err(e) => Err(Error::SerializationError(e)),
                    }
                })
            })
        } else {
            Err(Error::UnsupportedMethod)
        }
    }

    pub fn rhai_create(&mut self, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        let data = rhai::serde::from_dynamic(&data)?;
        let res = self.create(data).map_err(|e: Error| rhai_err(e))?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }

    pub fn replace(
        &self,
        name: &str,
        data: serde_json::Map<String, serde_json::Value>,
    ) -> Result<DynamicObject> {
        if let Some(api) = self.api.clone() {
            let mut handle = data.clone();
            if let Some(labels) = get_labels() {
                if !handle["metadata"].as_object().unwrap().contains_key("labels") {
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("labels".to_string(), json!({}));
                } else if !handle["metadata"].as_object_mut().unwrap()["labels"].is_object() {
                    handle["metadata"].as_object_mut().unwrap().remove_entry("labels");
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("labels".to_string(), json!({}));
                }
                for (k, v) in labels.as_object().unwrap() {
                    if !handle["metadata"].as_object_mut().unwrap()["labels"]
                        .as_object_mut()
                        .unwrap()
                        .keys()
                        .any(|name| name == k)
                    {
                        handle["metadata"].as_object_mut().unwrap()["labels"]
                            .as_object_mut()
                            .unwrap()
                            .insert(k.to_string(), v.clone());
                    }
                }
            }
            if self.scope == Scope::Namespaced
                && let Some(owner) = get_owner()
                && let Some(ns) = get_owner_ns()
                && let Some(mine) = self.ns.clone()
                && ns == mine
            {
                if handle["metadata"]
                    .as_object()
                    .unwrap()
                    .contains_key("ownerReferences")
                {
                    handle["metadata"].as_object_mut().unwrap()["ownerReferences"]
                        .as_array_mut()
                        .unwrap()
                        .push(owner);
                } else {
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("ownerReferences".to_string(), vec![owner].into());
                }
            }
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    match serde_json::from_value(handle.into()) {
                        Ok(obj) => api
                            .replace(name, &PostParams::default(), &obj)
                            .await
                            .map_err(Error::KubeError),
                        Err(e) => Err(Error::SerializationError(e)),
                    }
                })
            })
        } else {
            Err(Error::UnsupportedMethod)
        }
    }

    pub fn rhai_replace(&mut self, name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        let data = rhai::serde::from_dynamic(&data)?;
        let res = self.replace(&name, data).map_err(|e: Error| rhai_err(e))?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }

    pub fn patch(
        &self,
        name: &str,
        patch: serde_json::Map<String, serde_json::Value>,
    ) -> Result<DynamicObject> {
        if let Some(api) = self.api.clone() {
            let mut handle = patch.clone();
            if !handle.contains_key("metadata") || !handle["metadata"].is_object() {
                handle.insert("metadata".to_string(), json!({}));
            }
            if let Some(labels) = get_labels() {
                if !handle["metadata"].as_object().unwrap().contains_key("labels") {
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("labels".to_string(), json!({}));
                } else if !handle["metadata"].as_object_mut().unwrap()["labels"].is_object() {
                    handle["metadata"].as_object_mut().unwrap().remove_entry("labels");
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("labels".to_string(), json!({}));
                }
                for (k, v) in labels.as_object().unwrap() {
                    if !handle["metadata"].as_object_mut().unwrap()["labels"]
                        .as_object_mut()
                        .unwrap()
                        .keys()
                        .any(|name| name == k)
                    {
                        handle["metadata"].as_object_mut().unwrap()["labels"]
                            .as_object_mut()
                            .unwrap()
                            .insert(k.to_string(), v.clone());
                    }
                }
            }
            if self.scope == Scope::Namespaced
                && let Some(owner) = get_owner()
                && let Some(ns) = get_owner_ns()
                && let Some(mine) = self.ns.clone()
                && ns == mine
            {
                if handle["metadata"]
                    .as_object()
                    .unwrap()
                    .contains_key("ownerReferences")
                {
                    handle["metadata"].as_object_mut().unwrap()["ownerReferences"]
                        .as_array_mut()
                        .unwrap()
                        .push(owner);
                } else {
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("ownerReferences".to_string(), vec![owner].into());
                }
            }
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    api.patch(
                        name,
                        &PatchParams::apply(&get_client_name()).force(),
                        &Patch::Apply(handle),
                    )
                    .await
                    .map_err(Error::KubeError)
                })
            })
        } else {
            Err(Error::UnsupportedMethod)
        }
    }

    pub fn rhai_patch(&mut self, name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        let data = rhai::serde::from_dynamic(&data)?;
        let res = self.patch(&name, data).map_err(|e: Error| rhai_err(e))?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }

    pub fn apply(
        &self,
        name: &str,
        patch: serde_json::Map<String, serde_json::Value>,
    ) -> Result<DynamicObject> {
        if let Some(api) = self.api.clone() {
            let mut handle = patch.clone();
            if !handle.contains_key("metadata") || !handle["metadata"].is_object() {
                handle.insert("metadata".to_string(), json!({}));
            }
            if let Some(labels) = get_labels() {
                if !handle["metadata"].as_object().unwrap().contains_key("labels") {
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("labels".to_string(), json!({}));
                } else if !handle["metadata"].as_object_mut().unwrap()["labels"].is_object() {
                    handle["metadata"].as_object_mut().unwrap().remove_entry("labels");
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("labels".to_string(), json!({}));
                }
                for (k, v) in labels.as_object().unwrap() {
                    if !handle["metadata"].as_object_mut().unwrap()["labels"]
                        .as_object_mut()
                        .unwrap()
                        .keys()
                        .any(|name| name == k)
                    {
                        handle["metadata"].as_object_mut().unwrap()["labels"]
                            .as_object_mut()
                            .unwrap()
                            .insert(k.to_string(), v.clone());
                    }
                }
            }
            if self.scope == Scope::Namespaced
                && let Some(owner) = get_owner()
                && let Some(ns) = get_owner_ns()
                && let Some(mine) = self.ns.clone()
                && ns == mine
            {
                if handle["metadata"]
                    .as_object()
                    .unwrap()
                    .contains_key("ownerReferences")
                {
                    handle["metadata"].as_object_mut().unwrap()["ownerReferences"]
                        .as_array_mut()
                        .unwrap()
                        .push(owner);
                } else {
                    handle["metadata"]
                        .as_object_mut()
                        .unwrap()
                        .insert("ownerReferences".to_string(), vec![owner].into());
                }
            }
            let kind = patch
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("")
                .to_string();
            let api_for_get = api.clone();
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    match api
                        .patch(
                            name,
                            &PatchParams::apply(&get_client_name()).force(),
                            &Patch::Apply(handle),
                        )
                        .await
                    {
                        Ok(obj) => Ok(obj),
                        Err(e) => {
                            // SSA force on a completed Job fails with 422 "immutable" because
                            // Kubernetes adds controller-uid/job-name to spec.template.metadata.labels
                            // at runtime. If the Job is already complete the error is benign.
                            if kind == "Job"
                                && e.to_string().contains("immutable")
                                && let Ok(current) = api_for_get.get(name).await
                                && job_is_completed(&current.data)
                            {
                                tracing::debug!(
                                    "Job {name} spec.template immutable but already completed — skipping"
                                );
                                return Ok(current);
                            }
                            Err(Error::KubeError(e))
                        }
                    }
                })
            })
        } else {
            Err(Error::UnsupportedMethod)
        }
    }

    pub fn rhai_apply(&mut self, name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        let data = rhai::serde::from_dynamic(&data)?;
        let res = self.apply(&name, data).map_err(|e: Error| rhai_err(e))?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }
}

#[macro_export]
macro_rules! register_k8s_object {
    ($engine:expr, $type:ty) => {{
        let _delete: fn(&mut $type) -> $crate::RhaiRes<()> = <$type>::rhai_delete;
        let _wait_deleted: fn(&mut $type, i64) -> $crate::RhaiRes<()> = <$type>::rhai_wait_deleted;
        let _get_kind: fn(&mut $type) -> String = <$type>::get_kind;
        let _original_kind: fn(&mut $type) -> String = <$type>::original_kind;
        let _get_metadata: fn(&mut $type) -> $crate::RhaiRes<rhai::Dynamic> = <$type>::get_metadata;
        let _wait_condition: fn(&mut $type, String, i64) -> $crate::RhaiRes<()> = <$type>::wait_condition;
        let _wait_status: fn(&mut $type, String, i64) -> $crate::RhaiRes<()> = <$type>::wait_status;
        let _wait_status_prop: fn(&mut $type, String, i64) -> $crate::RhaiRes<()> = <$type>::wait_status_prop;
        let _wait_status_string: fn(&mut $type, String, String, i64) -> $crate::RhaiRes<()> =
            <$type>::wait_status_string;

        $engine
            .register_type_with_name::<$type>("K8sObject")
            .register_get("kind", _get_kind)
            .register_get("original_kind", _original_kind)
            .register_get("metadata", _get_metadata)
            .register_fn("delete", _delete)
            .register_fn("wait_condition", _wait_condition)
            .register_fn("wait_status", _wait_status)
            .register_fn("wait_status_prop", _wait_status_prop)
            .register_fn("wait_status_string", _wait_status_string)
            .register_fn("wait_deleted", _wait_deleted)
    }};
}

#[macro_export]
macro_rules! register_k8s_generic {
    ($engine:expr, $type:ty, $obj_type:ty,
     $new_global:expr, $new_ns:expr, $new_group_ns:expr) => {{
        let _scope: fn(&mut $type) -> String = <$type>::rhai_get_scope;
        let _exist: fn(&mut $type) -> $crate::RhaiRes<rhai::Dynamic> = <$type>::rhai_exist;
        let _list: fn(&mut $type) -> $crate::RhaiRes<rhai::Dynamic> = <$type>::rhai_list;
        let _list_labels: fn(&mut $type, String) -> $crate::RhaiRes<rhai::Dynamic> =
            <$type>::rhai_list_labels;
        let _list_meta: fn(&mut $type) -> $crate::RhaiRes<rhai::Dynamic> = <$type>::rhai_list_meta;
        let _get: fn(&mut $type, String) -> $crate::RhaiRes<rhai::Dynamic> = <$type>::rhai_get;
        let _get_meta: fn(&mut $type, String) -> $crate::RhaiRes<rhai::Dynamic> = <$type>::rhai_get_meta;
        let _get_obj: fn(&mut $type, String) -> $crate::RhaiRes<$obj_type> = <$type>::rhai_get_obj;
        let _delete: fn(&mut $type, String) -> $crate::RhaiRes<()> = <$type>::rhai_delete;
        let _create: fn(&mut $type, rhai::Dynamic) -> $crate::RhaiRes<rhai::Dynamic> = <$type>::rhai_create;
        let _replace: fn(&mut $type, String, rhai::Dynamic) -> $crate::RhaiRes<rhai::Dynamic> =
            <$type>::rhai_replace;
        let _patch: fn(&mut $type, String, rhai::Dynamic) -> $crate::RhaiRes<rhai::Dynamic> =
            <$type>::rhai_patch;
        let _apply: fn(&mut $type, String, rhai::Dynamic) -> $crate::RhaiRes<rhai::Dynamic> =
            <$type>::rhai_apply;

        $engine
            .register_type_with_name::<$type>("K8sGeneric")
            .register_fn("k8s_resource", $new_global)
            .register_fn("k8s_resource", $new_ns)
            .register_fn("k8s_resource", $new_group_ns)
            .register_fn("list", _list)
            .register_fn("list", _list_labels)
            .register_fn("update_k8s_crd_cache", update_cache)
            .register_fn("list_meta", _list_meta)
            .register_fn("get", _get)
            .register_fn("get_meta", _get_meta)
            .register_fn("get_obj", _get_obj)
            .register_fn("delete", _delete)
            .register_fn("create", _create)
            .register_fn("replace", _replace)
            .register_fn("patch", _patch)
            .register_fn("apply", _apply)
            .register_fn("exist", _exist)
            .register_get("scope", _scope)
    }};
}

pub fn k8sgeneric_rhai_register(engine: &mut Engine) {
    engine
        .register_type_with_name::<DynamicObject>("DynamicObject")
        .register_get("data", |obj: &mut DynamicObject| -> Dynamic {
            Dynamic::from(obj.data.clone())
        });
    register_k8s_object!(engine, K8sObject);
    register_k8s_generic!(
        engine,
        K8sGeneric,
        K8sObject,
        K8sGeneric::new_global,
        K8sGeneric::new_ns,
        K8sGeneric::new_group_ns
    );
}

fn job_is_completed(data: &serde_json::Value) -> bool {
    let status = match data.get("status") {
        Some(s) => s,
        None => return false,
    };
    if status.get("succeeded").and_then(|v| v.as_i64()).unwrap_or(0) > 0 {
        return true;
    }
    status.get("completionTime").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_k8s_object_compiles_for_k8sobject() {
        let mut engine = rhai::Engine::new();
        register_k8s_object!(engine, K8sObject);
    }

    #[test]
    fn register_k8s_generic_compiles_for_real() {
        let mut engine = rhai::Engine::new();
        register_k8s_generic!(
            engine,
            K8sGeneric,
            K8sObject,
            K8sGeneric::new_global,
            K8sGeneric::new_ns,
            K8sGeneric::new_group_ns
        );
    }

    #[test]
    fn test_job_is_completed_succeeded() {
        let data = serde_json::json!({"status": {"succeeded": 1}});
        assert!(job_is_completed(&data));
    }

    #[test]
    fn test_job_is_completed_zero_succeeded() {
        let data = serde_json::json!({"status": {"succeeded": 0}});
        assert!(!job_is_completed(&data));
    }

    #[test]
    fn test_job_is_completed_completion_time() {
        let data = serde_json::json!({"status": {"completionTime": "2024-01-01T00:00:00Z"}});
        assert!(job_is_completed(&data));
    }

    #[test]
    fn test_job_is_completed_no_status() {
        let data = serde_json::json!({});
        assert!(!job_is_completed(&data));
    }

    #[test]
    fn test_job_is_completed_still_running() {
        let data = serde_json::json!({"status": {"active": 1, "succeeded": 0}});
        assert!(!job_is_completed(&data));
    }
}
