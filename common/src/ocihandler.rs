use crate::{Error, Result, RhaiRes, rhai_err, rhaihandler::Map};
use chrono::Utc;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use oci_client::{Client, Reference, client, config, manifest, secrets::RegistryAuth};
use rhai::{Dynamic, Engine};
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
        let client = Client::new(client::ClientConfig::default());
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
        let layer = client::ImageLayer::oci_v1_gzip(data, None);
        let cfg = config::ConfigFile {
            created: Some(Utc::now()),
            architecture: config::Architecture::None,
            os: config::Os::Linux,
            config: Some(config::Config {
                working_dir: Some("/".into()),
                ..Default::default()
            }),
            history: Some(vec![config::History {
                author: None,
                created: Some(Utc::now()),
                created_by: Some("vynil".into()),
                comment: Some("vynil.build".into()),
                empty_layer: Some(false),
            }]),
            ..Default::default()
        };
        let config = client::Config::oci_v1_from_config_file(cfg, None)
            .map_err(Error::OCIDistrib)
            .map_err(rhai_err)?;
        let layers = vec![layer];
        let mut manifest = manifest::OciImageManifest::build(&layers, &config, Some(values));
        manifest.media_type = Some(manifest::OCI_IMAGE_MEDIA_TYPE.to_string());
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
        let client = Client::new(client::ClientConfig::default());
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
        let client = Client::new(client::ClientConfig::default());
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
        let client = Client::new(client::ClientConfig::default());
        let image = Reference::with_tag(self.registry.clone(), repository, tag);
        let (manifest, _) = block_in_place(|| {
            Handle::current().block_on(async move { client.pull_manifest(&image, &self.auth.clone()).await })
        })
        .map_err(|e| rhai_err(Error::OCIDistrib(e)))?;
        let v = serde_json::to_string(&manifest).map_err(|e| rhai_err(Error::SerializationError(e)))?;
        serde_json::from_str(&v).map_err(|e| rhai_err(Error::SerializationError(e)))
    }
}

pub fn oci_rhai_register(engine: &mut Engine) {
    engine
            .register_type_with_name::<Registry>("Registry")
            .register_fn("new_registry", Registry::new)
            .register_fn("push_image", Registry::push_image)
            .register_fn("list_tags", Registry::rhai_list_tags)
            .register_fn("get_manifest", Registry::get_manifest);
}
