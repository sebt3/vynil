use crate::{Error, Result, RhaiRes, context::get_client, rhai_err};
use kube::Client;
use rhai::{Dynamic, Engine};

lazy_static::lazy_static! {
    pub static ref CLIENT: Client = get_client();
}

#[derive(Clone)]
pub struct K8sRaw {
    pub client: Client,
}

impl K8sRaw {
    pub fn new() -> Self {
        Self {
            client: CLIENT.clone(),
        }
    }

    pub async fn get_url(&self, url: String) -> Result<serde_json::Value> {
        let req = http::Request::get(url)
            .body(Default::default())
            .map_err(Error::RawHTTP)?;
        let resp = self
            .client
            .request::<serde_json::Value>(req)
            .await
            .map_err(Error::KubeError)?;
        Ok(resp)
    }

    pub async fn get_url_as_disco(&self, url: String) -> Result<serde_json::Value> {
        let req = http::Request::get(url)
            .header("Accept", "application/json;g=apidiscovery.k8s.io;v=v2;as=APIGroupDiscoveryList,application/json;g=apidiscovery.k8s.io;v=v2beta1;as=APIGroupDiscoveryList,application/json")
            .body(Default::default())
            .map_err(Error::RawHTTP)?;
        let resp = self
            .client
            .request::<serde_json::Value>(req)
            .await
            .map_err(Error::KubeError)?;
        Ok(resp)
    }

    pub async fn get_api_version(&self) -> Result<serde_json::Value> {
        self.get_url("/version".to_string()).await
    }

    pub async fn get_api_resources(&self) -> Result<serde_json::Value> {
        self.get_url_as_disco("/apis".to_string()).await
    }

    pub fn rhai_get_url(&mut self, url: String) -> RhaiRes<Dynamic> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let res = self.get_url(url).await.map_err(rhai_err)?;
                let v = serde_json::to_string(&res)
                    .map_err(Error::SerializationError)
                    .map_err(rhai_err)?;
                serde_json::from_str(&v)
                    .map_err(Error::SerializationError)
                    .map_err(rhai_err)
            })
        })
    }

    pub fn rhai_get_api_version(&mut self) -> RhaiRes<Dynamic> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let ver = self.get_api_version().await.map_err(rhai_err)?;
                let v = serde_json::to_string(&ver)
                    .map_err(Error::SerializationError)
                    .map_err(rhai_err)?;
                serde_json::from_str(&v)
                    .map_err(Error::SerializationError)
                    .map_err(rhai_err)
            })
        })
    }

    pub fn rhai_get_api_resources(&mut self) -> RhaiRes<Dynamic> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let ver = self.get_api_resources().await.map_err(rhai_err)?;
                let v = serde_json::to_string(&ver)
                    .map_err(Error::SerializationError)
                    .map_err(rhai_err)?;
                serde_json::from_str(&v)
                    .map_err(Error::SerializationError)
                    .map_err(rhai_err)
            })
        })
    }
}

pub fn k8sraw_rhai_register(engine: &mut Engine) {
    engine
            .register_type_with_name::<K8sRaw>("K8sRaw")
            .register_fn("new_k8s_raw", K8sRaw::new)
            .register_fn("get_url", K8sRaw::rhai_get_url)
            .register_fn("get_api_resources", K8sRaw::rhai_get_api_resources)
            .register_fn("get_cluster_version", K8sRaw::rhai_get_api_version);
}
