use anyhow::Result;
use kube::{
    api::{Api, ListParams, ObjectList},
    Client,
};

use crate::{distrib::Distrib, install::Install};
pub struct InstallHandler {
    api: Api<Install>,
}
impl InstallHandler {
    #[must_use] pub fn new(cl: &Client, ns: &str) -> InstallHandler {
        InstallHandler {
            api: Api::namespaced(cl.clone(), ns),
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
    pub async fn list(&mut self) -> Result<ObjectList<Install>, kube::Error> {
        let lp = ListParams::default();
        self.api.list(&lp).await
    }
}

pub struct DistribHandler {
    api: Api<Distrib>,
}
impl DistribHandler {
    #[must_use] pub fn new(cl: &Client) -> DistribHandler {
        DistribHandler {
            api: Api::all(cl.clone()),
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
    pub async fn list(&mut self) -> Result<ObjectList<Distrib>, kube::Error> {
        let lp = ListParams::default();
        self.api.list(&lp).await
    }
}

pub use k8s_openapi::api::networking::v1::Ingress;
pub struct IngressHandler {
    api: Api<Ingress>,
}
impl IngressHandler {
    #[must_use] pub fn new(cl: &Client, ns: &str) -> IngressHandler {
        IngressHandler {
            api: Api::namespaced(cl.clone(), ns),
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
    pub async fn list(&mut self) -> Result<ObjectList<Ingress>, kube::Error> {
        let lp = ListParams::default();
        self.api.list(&lp).await
    }
}

pub use k8s_openapi::api::core::v1::Secret;
pub struct SecretHandler {
    api: Api<Secret>,
}
impl SecretHandler {
    #[must_use] pub fn new(cl: &Client, ns: &str) -> SecretHandler {
        SecretHandler {
            api: Api::namespaced(cl.clone(), ns),
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
    pub async fn list(&mut self) -> Result<ObjectList<Secret>, kube::Error> {
        let lp = ListParams::default();
        self.api.list(&lp).await
    }
}

pub use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
pub struct CustomResourceDefinitionHandler {
    api: Api<CustomResourceDefinition>,
}
impl CustomResourceDefinitionHandler {
    #[must_use] pub fn new(cl: &Client) -> CustomResourceDefinitionHandler {
        CustomResourceDefinitionHandler {
            api: Api::all(cl.clone()),
        }
    }
    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for crd in list {
            if crd.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }
    pub async fn get(&mut self, name: &str) -> Result<CustomResourceDefinition, kube::Error> {
        self.api.get(name).await
    }
    pub async fn list(&mut self) -> Result<ObjectList<CustomResourceDefinition>, kube::Error> {
        let lp = ListParams::default();
        self.api.list(&lp).await
    }
}

pub use k8s_openapi::api::core::v1::Service;
pub struct ServiceHandler {
    api: Api<Service>,
}
impl ServiceHandler {
    #[must_use] pub fn new(cl: &Client, ns: &str) -> ServiceHandler {
        ServiceHandler {
            api: Api::namespaced(cl.clone(), ns),
        }
    }
    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for svc in list {
            if svc.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }
    pub async fn get(&mut self, name: &str) -> Result<Service, kube::Error> {
        self.api.get(name).await
    }
    pub async fn list(&mut self) -> Result<ObjectList<Service>, kube::Error> {
        let lp = ListParams::default();
        self.api.list(&lp).await
    }
}

pub use k8s_openapi::api::core::v1::Namespace;
pub struct NamespaceHandler {
    api: Api<Namespace>,
}
impl NamespaceHandler {
    #[must_use] pub fn new(cl: &Client) -> NamespaceHandler {
        NamespaceHandler {
            api: Api::all(cl.clone()),
        }
    }
    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for ns in list {
            if ns.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }
    pub async fn get(&mut self, name: &str) -> Result<Namespace, kube::Error> {
        self.api.get(name).await
    }
    pub async fn list(&mut self) -> Result<ObjectList<Namespace>, kube::Error> {
        let lp = ListParams::default();
        self.api.list(&lp).await
    }
}

pub use k8s_openapi::api::storage::v1::StorageClass;
pub struct StorageClassHandler {
    api: Api<StorageClass>,
}
impl StorageClassHandler {
    #[must_use] pub fn new(cl: &Client) -> StorageClassHandler {
        StorageClassHandler {
            api: Api::all(cl.clone()),
        }
    }
    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for sc in list {
            if sc.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }
    pub async fn get(&mut self, name: &str) -> Result<StorageClass, kube::Error> {
        self.api.get(name).await
    }
    pub async fn list(&mut self) -> Result<ObjectList<StorageClass>, kube::Error> {
        let lp = ListParams::default();
        self.api.list(&lp).await
    }
}

pub use k8s_openapi::api::storage::v1::CSIDriver;
pub struct CSIDriverHandler {
    api: Api<CSIDriver>,
}
impl CSIDriverHandler {
    #[must_use] pub fn new(cl: &Client) -> CSIDriverHandler {
        CSIDriverHandler {
            api: Api::all(cl.clone()),
        }
    }
    pub async fn have(&mut self, name: &str) -> bool {
        let lp = ListParams::default();
        let list = self.api.list(&lp).await.unwrap();
        for sc in list {
            if sc.metadata.name.clone().unwrap() == name {
                return true;
            }
        }
        false
    }
    pub async fn get(&mut self, name: &str) -> Result<CSIDriver, kube::Error> {
        self.api.get(name).await
    }
    pub async fn list(&mut self) -> Result<ObjectList<CSIDriver>, kube::Error> {
        let lp = ListParams::default();
        self.api.list(&lp).await
    }
}
