use crate::{
    context::get_client,
    rhai_err, Error, RhaiRes,
};
use k8s_openapi::api::{apps::v1::Deployment, batch::v1::Job};
use kube::{
    api::Api,
    discovery::Discovery, runtime::wait::{await_condition, conditions, Condition}, Client, ResourceExt
};
use rhai::Dynamic;

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
    pub static ref CACHE: Discovery = populate_cache();
}


#[derive(Clone, Debug)]
pub struct K8sDeploy {
    pub api: Api<Deployment>,
    pub obj: Deployment,
}
impl K8sDeploy {
    pub fn is_deploy_available() -> impl Condition<Deployment> {
        |obj: Option<&Deployment>| {
            tracing::warn!("Testing conditions");
            if let Some(job) = &obj {
                if let Some(s) = &job.status {
                    if let Some(conds) = &s.conditions {
                        if let Some(pcond) = conds.iter().find(|c| c.type_ == "Available") {
                            tracing::warn!(pcond.status);
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
            tokio::runtime::Handle::current().block_on(async move {
                api.get(&name)
                    .await
                    .map_err(|e| Error::KubeError(e))
            })
        }).map_err(|e| rhai_err(e))?;
        Ok(K8sDeploy {
            api: Api::namespaced(CLIENT.clone(), &namespace),
            obj: d
        })
    }
    pub fn get_metadata(&mut self)-> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.obj.metadata).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }
    pub fn get_spec(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.obj.spec).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }
    pub fn get_status(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.obj.status).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }
    pub fn wait_available(&mut self, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        tracing::warn!("wait_available {}", name);
        let cond = await_condition(self.api.clone(), &name, Self::is_deploy_available());
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond).await.map_err(|e| Error::Elapsed(e))
            })
        }).map_err(|e| rhai_err(e))?.map_err(|e| rhai_err(Error::KubeWaitError(e)))?;
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
            tokio::runtime::Handle::current().block_on(async move {
                api.get(&name)
                    .await
                    .map_err(|e| Error::KubeError(e))
            })
        }).map_err(|e| rhai_err(e))?;
        Ok(K8sJob {
            api: Api::namespaced(CLIENT.clone(), &namespace),
            obj: j
        })
    }
    pub fn get_metadata(&mut self)-> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.obj.metadata).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }
    pub fn get_spec(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.obj.spec).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }
    pub fn get_status(&mut self) -> RhaiRes<Dynamic> {
        let v = serde_json::to_string(&self.obj.status).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }
    pub fn wait_done(&mut self, timeout: i64) -> RhaiRes<()> {
        let name = self.obj.name_any();
        let cond = await_condition(self.api.clone(), &name, conditions::is_job_completed());
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), cond).await.map_err(|e| Error::Elapsed(e))
            })
        }).map_err(|e| rhai_err(e))?.map_err(|e| rhai_err(Error::KubeWaitError(e)))?;
        Ok(())
    }
}
