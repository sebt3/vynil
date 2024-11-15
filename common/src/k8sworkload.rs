use crate::{context::get_client, rhai_err, Error, RhaiRes};
use k8s_openapi::api::{
    apps::v1::{DaemonSet, Deployment, StatefulSet},
    batch::v1::Job,
};
use kube::{
    api::Api,
    runtime::wait::{await_condition, conditions, Condition},
    Client, ResourceExt,
};
use rhai::Dynamic;

lazy_static::lazy_static! {
    pub static ref CLIENT: Client = get_client();
}

#[derive(Clone, Debug)]
pub struct K8sDaemonSet {
    pub api: Api<DaemonSet>,
    pub obj: DaemonSet,
}
impl K8sDaemonSet {
    pub fn is_deamonset_available() -> impl Condition<DaemonSet> {
        |obj: Option<&DaemonSet>| {
            if let Some(ds) = &obj {
                if let Some(s) = &ds.status {
                    return s.desired_number_scheduled == s.number_available.unwrap_or(0);
                }
            }
            false
        }
    }

    pub fn get_deamonset(namespace: String, name: String) -> RhaiRes<K8sDaemonSet> {
        let api: Api<DaemonSet> = Api::namespaced(CLIENT.clone(), &namespace);
        let d = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { api.get(&name).await.map_err(Error::KubeError) })
        })
        .map_err(rhai_err)?;
        Ok(K8sDaemonSet {
            api: Api::namespaced(CLIENT.clone(), &namespace),
            obj: d,
        })
    }

    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        let v =
            serde_json::to_string(&self.obj.metadata).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_spec(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.obj.spec).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_status(&mut self) -> RhaiRes<Dynamic> {
        let v =
            serde_json::to_string(&self.obj.status).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn wait_available(&mut self, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        let cond = await_condition(self.api.clone(), &name, Self::is_deamonset_available());
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(Error::Elapsed)
            })
        })
        .map_err(rhai_err)?
        .map_err(|e| rhai_err(Error::KubeWaitError(e)))?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct K8sStatefulSet {
    pub api: Api<StatefulSet>,
    pub obj: StatefulSet,
}
impl K8sStatefulSet {
    pub fn is_sts_available() -> impl Condition<StatefulSet> {
        |obj: Option<&StatefulSet>| {
            tracing::warn!("Testing conditions");
            if let Some(sts) = &obj {
                if let Some(spec) = &sts.spec {
                    if let Some(s) = &sts.status {
                        return spec.replicas.unwrap_or(1) == s.available_replicas.unwrap_or(0);
                    }
                }
            }
            false
        }
    }

    pub fn get_sts(namespace: String, name: String) -> RhaiRes<K8sStatefulSet> {
        let api: Api<StatefulSet> = Api::namespaced(CLIENT.clone(), &namespace);
        let d = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { api.get(&name).await.map_err(Error::KubeError) })
        })
        .map_err(rhai_err)?;
        Ok(K8sStatefulSet {
            api: Api::namespaced(CLIENT.clone(), &namespace),
            obj: d,
        })
    }

    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        let v =
            serde_json::to_string(&self.obj.metadata).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_spec(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.obj.spec).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_status(&mut self) -> RhaiRes<Dynamic> {
        let v =
            serde_json::to_string(&self.obj.status).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn wait_available(&mut self, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        let cond = await_condition(self.api.clone(), &name, Self::is_sts_available());
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(Error::Elapsed)
            })
        })
        .map_err(rhai_err)?
        .map_err(|e| rhai_err(Error::KubeWaitError(e)))?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct K8sDeploy {
    pub api: Api<Deployment>,
    pub obj: Deployment,
}
impl K8sDeploy {
    pub fn is_deploy_available() -> impl Condition<Deployment> {
        |obj: Option<&Deployment>| {
            if let Some(job) = &obj {
                if let Some(s) = &job.status {
                    if let Some(conds) = &s.conditions {
                        if let Some(pcond) = conds.iter().find(|c| c.type_ == "Available") {
                            return pcond.status == "True";
                        }
                    }
                }
            }
            false
        }
    }

    pub fn get_deployment(namespace: String, name: String) -> RhaiRes<K8sDeploy> {
        let api: Api<Deployment> = Api::namespaced(CLIENT.clone(), &namespace);
        let d = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { api.get(&name).await.map_err(Error::KubeError) })
        })
        .map_err(rhai_err)?;
        Ok(K8sDeploy {
            api: Api::namespaced(CLIENT.clone(), &namespace),
            obj: d,
        })
    }

    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        let v =
            serde_json::to_string(&self.obj.metadata).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_spec(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.obj.spec).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_status(&mut self) -> RhaiRes<Dynamic> {
        let v =
            serde_json::to_string(&self.obj.status).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn wait_available(&mut self, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        let cond = await_condition(self.api.clone(), &name, Self::is_deploy_available());
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(Error::Elapsed)
            })
        })
        .map_err(rhai_err)?
        .map_err(|e| rhai_err(Error::KubeWaitError(e)))?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct K8sJob {
    pub api: Api<Job>,
    pub obj: Job,
}
impl K8sJob {
    pub fn get_job(namespace: String, name: String) -> RhaiRes<K8sJob> {
        let api: Api<Job> = Api::namespaced(CLIENT.clone(), &namespace);
        let j = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async move { api.get(&name).await.map_err(Error::KubeError) })
        })
        .map_err(rhai_err)?;
        Ok(K8sJob {
            api: Api::namespaced(CLIENT.clone(), &namespace),
            obj: j,
        })
    }

    pub fn get_metadata(&mut self) -> RhaiRes<Dynamic> {
        let v =
            serde_json::to_string(&self.obj.metadata).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_spec(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.obj.spec).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn get_status(&mut self) -> RhaiRes<Dynamic> {
        let v =
            serde_json::to_string(&self.obj.status).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }

    pub fn wait_done(&mut self, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        let cond = await_condition(self.api.clone(), &name, conditions::is_job_completed());
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond)
                    .await
                    .map_err(Error::Elapsed)
            })
        })
        .map_err(rhai_err)?
        .map_err(|e| rhai_err(Error::KubeWaitError(e)))?;
        Ok(())
    }
}
