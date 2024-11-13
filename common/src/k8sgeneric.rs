use std::sync::Mutex;

use crate::{
    context::{get_client, get_client_name, get_labels, get_owner, get_owner_ns},
    rhai_err, Error, Result, RhaiRes,
};
use kube::{
    api::{
        Api, DeleteParams, DynamicObject, ListParams, ObjectList, PartialObjectMeta, Patch, PatchParams,
        PostParams,
    },
    discovery::{ApiCapabilities, ApiResource, Discovery, Scope},
    runtime::wait::{await_condition, conditions, Condition},
    Client, ResourceExt,
};
use rhai::{serde::to_dynamic, Dynamic};
use serde_json::json;

lazy_static::lazy_static! {
    pub static ref CLIENT: Client = get_client();
}
fn populate_cache() -> Discovery {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async move {
            Discovery::new(CLIENT.clone())
                .run()
                .await
                .expect("create discovery")
        })
    })
}
lazy_static::lazy_static! {
    pub static ref CACHE: Mutex<Discovery> = Mutex::new(populate_cache());
}

pub fn update_cache() {
    *CACHE.lock().unwrap() = populate_cache();
}

#[derive(Clone, Debug)]
pub struct K8sObject {
    pub api: Api<DynamicObject>,
    pub obj: PartialObjectMeta,
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
        .map_err(|e| rhai_err(e))
    }

    pub fn rhai_wait_deleted(&mut self, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        let uid = self.obj.uid().unwrap();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let cond = await_condition(self.api.clone(), &name, conditions::is_deleted(&uid));
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(|e| Error::Elapsed(e))
            })
        })
        .map_err(|e| rhai_err(e))?
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

    pub fn is_condition(cond: String) -> impl Condition<DynamicObject> {
        move |obj: Option<&DynamicObject>| {
            if let Some(dynobj) = &obj {
                if dynobj.data.is_object()
                    && dynobj
                        .data
                        .as_object()
                        .unwrap()
                        .keys()
                        .into_iter()
                        .collect::<Vec<&String>>()
                        .contains(&&"status".to_string())
                {
                    let status = dynobj.data.as_object().unwrap()["status"].clone();
                    if status.is_object()
                        && status
                            .as_object()
                            .unwrap()
                            .keys()
                            .into_iter()
                            .collect::<Vec<&String>>()
                            .contains(&&"conditions".to_string())
                    {
                        let conditions = status.as_object().unwrap()["conditions"].clone();
                        if conditions.is_array()
                            && conditions.as_array().unwrap().into_iter().any(|c| {
                                c.is_object()
                                    && c.as_object()
                                        .unwrap()
                                        .keys()
                                        .into_iter()
                                        .collect::<Vec<&String>>()
                                        .contains(&&"type".to_string())
                                    && c.as_object().unwrap()["type"].is_string()
                                    && c.as_object().unwrap()["type"].as_str().unwrap() == cond
                                    && c.as_object()
                                        .unwrap()
                                        .keys()
                                        .into_iter()
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
            }
            false
        }
    }

    pub fn wait_condition(&mut self, condition: String, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        tracing::info!("wait_condition({}) for {}", &condition, name);
        let cond = await_condition(self.api.clone(), &name, Self::is_condition(condition));
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(|e| Error::Elapsed(e))
            })
        })
        .map_err(|e| rhai_err(e))?
        .map_err(|e| rhai_err(Error::KubeWaitError(e)))?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct K8sGeneric {
    pub api: Option<Api<DynamicObject>>,
    pub ns: Option<String>,
    pub scope: Scope,
}

// TODO: scale et exec

impl K8sGeneric {
    #[must_use]
    pub fn new(name: &str, ns: Option<String>) -> K8sGeneric {
        if let Some((res, cap)) = CACHE
            .lock()
            .unwrap()
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
            }
        } else {
            K8sGeneric {
                api: None,
                ns: None,
                scope: Scope::Cluster,
            }
        }
    }

    pub fn new_ns(name: String, ns: String) -> K8sGeneric {
        K8sGeneric::new(&name.as_str(), Some(ns))
    }

    pub fn new_global(name: String) -> K8sGeneric {
        K8sGeneric::new(&name.as_str(), None)
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
        let res = self.list().map_err(|e| rhai_err(e))?;
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
        let res = self.list_meta().map_err(|e| rhai_err(e))?;
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
        let res = self.get(&name).map_err(|e| rhai_err(e))?;
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
        let res = self.get_meta(&name).map_err(|e| rhai_err(e))?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }

    pub fn rhai_get_obj(&mut self, name: String) -> RhaiRes<K8sObject> {
        let res = self.get_meta(&name).map_err(|e| rhai_err(e))?;
        Ok(K8sObject {
            api: self.api.clone().unwrap(),
            obj: res,
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
        self.delete(&name).map_err(|e| rhai_err(e))
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
                        .into_iter()
                        .any(|name| name == k)
                    {
                        handle["metadata"].as_object_mut().unwrap()["labels"]
                            .as_object_mut()
                            .unwrap()
                            .insert(k.to_string(), v.clone());
                    }
                }
            }
            if self.scope == Scope::Namespaced {
                if let Some(owner) = get_owner() {
                    if let Some(ns) = get_owner_ns() {
                        if let Some(mine) = self.ns.clone() {
                            if ns == mine {
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
                        }
                    }
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
                        .into_iter()
                        .any(|name| name == k)
                    {
                        handle["metadata"].as_object_mut().unwrap()["labels"]
                            .as_object_mut()
                            .unwrap()
                            .insert(k.to_string(), v.clone());
                    }
                }
            }
            if self.scope == Scope::Namespaced {
                if let Some(owner) = get_owner() {
                    if let Some(ns) = get_owner_ns() {
                        if let Some(mine) = self.ns.clone() {
                            if ns == mine {
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
                        }
                    }
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
                        .into_iter()
                        .any(|name| name == k)
                    {
                        handle["metadata"].as_object_mut().unwrap()["labels"]
                            .as_object_mut()
                            .unwrap()
                            .insert(k.to_string(), v.clone());
                    }
                }
            }
            if self.scope == Scope::Namespaced {
                if let Some(owner) = get_owner() {
                    if let Some(ns) = get_owner_ns() {
                        if let Some(mine) = self.ns.clone() {
                            if ns == mine {
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
                        }
                    }
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
                        .into_iter()
                        .any(|name| name == k)
                    {
                        handle["metadata"].as_object_mut().unwrap()["labels"]
                            .as_object_mut()
                            .unwrap()
                            .insert(k.to_string(), v.clone());
                    }
                }
            }
            if self.scope == Scope::Namespaced {
                if let Some(owner) = get_owner() {
                    if let Some(ns) = get_owner_ns() {
                        if let Some(mine) = self.ns.clone() {
                            if ns == mine {
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
                        }
                    }
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

    pub fn rhai_apply(&mut self, name: String, data: rhai::Dynamic) -> RhaiRes<Dynamic> {
        let data = rhai::serde::from_dynamic(&data)?;
        let res = self.apply(&name, data).map_err(|e: Error| rhai_err(e))?;
        let v = serde_json::to_value(res).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        to_dynamic(v)
    }
}
