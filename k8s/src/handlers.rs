use anyhow::Result;
use kube::{
    api::{Api, ListParams},
    Client,
};

use crate::{distrib::Distrib, install::Install};
pub struct InstallHandler {
    api: Api<Install>,
}
impl InstallHandler {
    #[must_use] pub fn new(cl: Client, ns: &str) -> InstallHandler {
        InstallHandler {
            api: Api::namespaced(cl, ns),
        }
    }
    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for inst in list {
            if inst.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }
    pub async fn get(&mut self, name: &str) -> Result<Install, kube::Error> {
        self.api.get(name).await
    }
}

pub struct DistribHandler {
    api: Api<Distrib>,
}
impl DistribHandler {
    #[must_use] pub fn new(cl: Client) -> DistribHandler {
        DistribHandler {
            api: Api::all(cl),
        }
    }
    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for dist in list {
            if dist.name().as_str() == name {
                return true;
            }
        }
        false
    }
    pub async fn get(&mut self, name: &str) -> Result<Distrib, kube::Error> {
        self.api.get(name).await
    }
}

use k8s_openapi::api::networking::v1::Ingress;
pub struct IngressHandler {
    api: Api<Ingress>,
}
impl IngressHandler {
    #[must_use] pub fn new(cl: Client, ns: &str) -> IngressHandler {
        IngressHandler {
            api: Api::namespaced(cl, ns),
        }
    }
    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for ing in list {
            if ing.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }
    pub async fn get(&mut self, name: &str) -> Result<Ingress, kube::Error> {
        self.api.get(name).await
    }
}

use k8s_openapi::api::core::v1::Secret;
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
        for secret in list {
            if secret.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }
    pub async fn get(&mut self, name: &str) -> Result<Secret, kube::Error> {
        self.api.get(name).await
    }
}
