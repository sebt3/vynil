use k8s_openapi::api::batch::v1::Job;
use kube::{
    api::{Api, DeleteParams, PostParams, ListParams, PatchParams, Patch},
    runtime::wait::{await_condition, conditions},
    Client,
};
use either::Either;

use crate::OPERATOR;

pub struct JobHandler {
    api: Api<Job>,
}

impl JobHandler {
    #[must_use] pub fn new(cl: Client, ns: &str) -> JobHandler {
        JobHandler {
            api: Api::namespaced(cl, ns),
        }
    }

    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for job in list {
            if job.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }

    pub async fn get(&mut self, name: &str) -> Result<Job, kube::Error> {
        self.api.get(name).await
    }

    pub async fn create(&mut self, name: &str, template: &serde_json::Value) -> Result<Job, kube::Error> {
        let data = serde_json::from_value(serde_json::json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {
                "name": name,
            },
            "spec": {
                "template": template
            }
        })).unwrap();
        self.api.create(&PostParams::default(), &data).await
    }

    pub async fn apply(&mut self, name: &str, template: &serde_json::Value) -> Result<Job, kube::Error> {
        if self.have(name).await {
            let params = PatchParams::apply(OPERATOR);
            let patch = Patch::Apply(serde_json::json!({
                "apiVersion": "batch/v1",
                "kind": "Job",
                "metadata": {
                    "name": name,
                },
                "spec": {
                    "template": template
                }
            }));
            self.api.patch(name, &params, &patch).await
        } else {
            self.create(name, template).await
        }
    }

    pub async fn wait(&mut self, name: &str) -> Result<Result<Option<Job>, kube::runtime::wait::Error>, tokio::time::error::Elapsed> {
        self.wait_max(name, 20).await
    }
    pub async fn wait_max(&mut self, name: &str, secs: u64) -> Result<Result<Option<Job>, kube::runtime::wait::Error>, tokio::time::error::Elapsed> {
        let cond = await_condition(self.api.clone(), name, conditions::is_job_completed());
        tokio::time::timeout(std::time::Duration::from_secs(secs), cond).await
    }
    pub async fn delete(&mut self, name: &str) -> Result<Either<Job, kube::core::response::Status>, kube::Error> {
        self.api.delete(name, &DeleteParams::background()).await
    }
}
