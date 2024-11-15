use crate::{rhai_err, rhaihandler::Map, Error, Result, RhaiRes};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use oci_client::{
    client::{ClientConfig, Config, ImageLayer},
    manifest,
    secrets::RegistryAuth,
    Client, Reference,
};
use rhai::Dynamic;
use std::{collections::BTreeMap, path::PathBuf};
use tar::{Archive, Builder};
use tokio::{runtime::Handle, task::block_in_place};

#[derive(Clone, Debug)]
pub struct Registry {
    auth: RegistryAuth,
    registry: String,
}
impl Registry {
    #[must_use]
    pub fn new(registry: String, username: String, password: String) -> Self {
        Self {
            auth: if username.is_empty() || password.is_empty() {
                RegistryAuth::Anonymous
            } else {
                RegistryAuth::Basic(username, password)
            },
            registry,
        }
    }

    pub fn push_image(
        &mut self,
        source_dir: String,
        repository: String,
        tag: String,
        annotations: Map,
    ) -> RhaiRes<()> {
        let client = Client::new(ClientConfig::default());
        let reference = Reference::with_tag(self.registry.clone(), repository, tag);
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        for (key, val) in annotations {
            values.insert(key.into(), val.to_string());
        }
        let mut tar = Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        tar.append_dir_all(".", source_dir)
            .map_err(|e| rhai_err(Error::Stdio(e)))?;
        let encoded = tar.into_inner().map_err(|e| rhai_err(Error::Stdio(e)))?;
        let data = encoded.finish().map_err(|e| rhai_err(Error::Stdio(e)))?;
        let layers = vec![ImageLayer::new(
            data,
            manifest::IMAGE_LAYER_GZIP_MEDIA_TYPE.to_string(),
            None,
        )];
        let config = Config {
            data: b"{}".to_vec(),
            media_type: manifest::IMAGE_CONFIG_MEDIA_TYPE.to_string(),
            annotations: None,
        };
        let manifest = manifest::OciImageManifest::build(&layers, &config, Some(values));
        block_in_place(|| {
            Handle::current().block_on(async move {
                client
                    .push(&reference, &layers, config, &self.auth.clone(), Some(manifest))
                    .await
            })
        })
        .map_err(|e| rhai_err(Error::OCIDistrib(e)))?;
        Ok(())
    }

    pub fn pull_image(&mut self, dest_dir: &PathBuf, repository: String, tag: String) -> Result<()> {
        let client = Client::new(ClientConfig::default());
        let reference = Reference::with_tag(self.registry.clone(), repository, tag);
        let data = block_in_place(|| {
            Handle::current().block_on(async move {
                client
                    .pull(&reference, &self.auth.clone(), vec![
                        manifest::IMAGE_LAYER_GZIP_MEDIA_TYPE,
                        manifest::IMAGE_DOCKER_LAYER_GZIP_MEDIA_TYPE,
                    ])
                    .await
            })
        })
        .map_err(Error::OCIDistrib)?;
        for layer in data.layers {
            let mut archive = Archive::new(GzDecoder::new(&layer.data[..]));
            archive.unpack(dest_dir).map_err(Error::Stdio)?;
        }
        Ok(())
    }

    pub async fn list_tags(&mut self, repository: String) -> Result<Vec<String>> {
        let client = Client::new(ClientConfig::default());
        let image: Reference = format!("{}/{}", self.registry.clone(), repository)
            .parse()
            .map_err(Error::OCIParseError)?;
        let ret = client
            .list_tags(&image, &self.auth.clone(), Some(100), None)
            .await
            .map_err(Error::OCIDistrib)?;
        Ok(ret.tags)
    }

    pub fn rhai_list_tags(&mut self, repository: String) -> RhaiRes<Dynamic> {
        block_in_place(|| Handle::current().block_on(async move { self.list_tags(repository).await }))
            .map_err(rhai_err)
            .map(|lst| lst.into_iter().collect())
    }

    pub fn get_manifest(&mut self, repository: String, tag: String) -> RhaiRes<Dynamic> {
        let client = Client::new(ClientConfig::default());
        let image = Reference::with_tag(self.registry.clone(), repository, tag);
        let (manifest, _) = block_in_place(|| {
            Handle::current().block_on(async move { client.pull_manifest(&image, &self.auth.clone()).await })
        })
        .map_err(|e| rhai_err(Error::OCIDistrib(e)))?;
        let v = serde_json::to_string(&manifest).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }
}
