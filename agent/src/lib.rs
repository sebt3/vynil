pub use k8s::{distrib::Distrib, install::Install, events};
use kube::{
    api::{Api, ListParams},
    Client,
};

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
        for dist in list {
            if dist.name().as_str() == name {
                return true;
            }
        }
        false
    }
    pub async fn get(&mut self, name: &str) -> Result<Install, kube::Error> {
        self.api.get(name).await
    }
}

pub async fn get_client() -> Client {
    Client::try_default().await.expect("create client")
}

pub static AGENT: &str = "operator.vynil.solidite.fr";
