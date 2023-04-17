use crate::OPERATOR;
use k8s_openapi::api::core::v1::PersistentVolumeClaim;
use kube::{
    api::{Api, DeleteParams, PostParams, ListParams, PatchParams, Patch},
    Client,
};
use either::Either;

pub struct PersistentVolumeClaimHandler {
    api: Api<PersistentVolumeClaim>,
}

impl PersistentVolumeClaimHandler {
    #[must_use] pub fn new(cl: Client, ns: &str) -> PersistentVolumeClaimHandler {
        PersistentVolumeClaimHandler {
            api: Api::namespaced(cl, ns),
        }
    }

    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for pvc in list {
            if pvc.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }

    pub async fn get(&mut self, name: &str) -> Result<PersistentVolumeClaim, kube::Error> {
        self.api.get(name).await
    }

    pub async fn create(&mut self, name: &str, spec: &serde_json::Value) -> Result<PersistentVolumeClaim, kube::Error> {
        let data = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1",
            "kind": "PersistentVolumeClaim",
            "metadata": {
                "name": name,
            },
            "spec": spec
        })).unwrap();
        self.api.create(&PostParams::default(), &data).await
    }

    pub async fn apply(&mut self, name: &str, spec: &serde_json::Value) -> Result<PersistentVolumeClaim, kube::Error> {
        if self.have(name).await {
            let params = PatchParams::apply(OPERATOR);
            let patch = Patch::Apply(serde_json::json!({
                "apiVersion": "v1",
                "kind": "PersistentVolumeClaim",
                "metadata": {
                    "name": name,
                },
                "spec": spec
            }));
            self.api.patch(name, &params, &patch).await
        } else {
            self.create(name, spec).await
        }
    }

    pub async fn delete(&mut self, name: &str) -> Result<Either<PersistentVolumeClaim, kube::core::response::Status>, kube::Error> {
        self.api.delete(name, &DeleteParams::background()).await
    }
}
