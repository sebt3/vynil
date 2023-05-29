use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{Api, DeleteParams, PostParams, ListParams, PatchParams, Patch},
    Client,
};
use either::Either;

use crate::OPERATOR;

pub struct SecretHandler {
    api: Api<Secret>,
}

impl SecretHandler {
    #[must_use] pub fn new(cl: Client, ns: &str) -> SecretHandler {
        SecretHandler {
            api: Api::namespaced(cl, ns),
        }
    }

    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for scrt in list {
            if scrt.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }

    pub async fn get(&mut self, name: &str) -> Result<Secret, kube::Error> {
        self.api.get(name).await
    }

    pub async fn create(&mut self, name: &str, spec: &serde_json::Value) -> Result<Secret, kube::Error> {
        let data = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": {
                "name": name,
            },
            "data": spec
        })).unwrap();
        self.api.create(&PostParams::default(), &data).await
    }

    pub async fn apply(&mut self, name: &str, spec: &serde_json::Value) -> Result<Secret, kube::Error> {
        if self.have(name).await {
            let params = PatchParams::apply(OPERATOR);
            let patch = Patch::Apply(serde_json::json!({
                "apiVersion": "v1",
                "kind": "Secret",
                "metadata": {
                    "name": name,
                },
                "data": spec
            }));
            self.api.patch(name, &params, &patch).await
        } else {
            self.create(name, spec).await
        }
    }

    pub async fn delete(&mut self, name: &str) -> Result<Either<Secret, kube::core::response::Status>, kube::Error> {
        self.api.delete(name, &DeleteParams::background()).await
    }
}
