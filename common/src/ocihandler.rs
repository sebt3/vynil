use base64::Engine as _;
use crate::{Error, Result, RhaiRes, rhai_err, rhaihandler::Map};
use chrono::Utc;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use k8s_openapi::api::core::v1::Secret;
use kube::{Client as KubeClient, api::Api};
use oci_client::{Client, Reference, client, config, manifest, secrets::RegistryAuth};
pub use oci_client::secrets::RegistryAuth as OciRegistryAuth;
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
        // Build uncompressed tar first to compute the diff_id (sha256 of uncompressed content)
        let mut tar_uncompressed = Builder::new(Vec::new());
        tar_uncompressed
            .append_dir_all(".", source_dir)
            .map_err(|e| rhai_err(Error::Stdio(e)))?;
        let raw_tar = tar_uncompressed.into_inner().map_err(|e| rhai_err(Error::Stdio(e)))?;
        let diff_id = format!("sha256:{}", sha256::digest(raw_tar.as_slice()));
        // Gzip compress for the actual layer
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        std::io::copy(&mut raw_tar.as_slice(), &mut gz).map_err(|e| rhai_err(Error::Stdio(e)))?;
        let data = gz.finish().map_err(|e| rhai_err(Error::Stdio(e)))?;
        let layer = client::ImageLayer::oci_v1_gzip(data, None);
        let cfg = config::ConfigFile {
            created: Some(Utc::now()),
            architecture: config::Architecture::None,
            os: config::Os::Linux,
            rootfs: config::Rootfs {
                r#type: "layers".to_string(),
                diff_ids: vec![diff_id],
            },
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

/// Lit un secret k8s de type dockerconfigjson et retourne les credentials OCI
/// pour le registre demandé. Retourne Anonymous si le secret est absent ou ne
/// contient pas d'entrée pour ce registre.
pub async fn resolve_registry_auth(
    secret_name: &str,
    registry: &str,
    client: KubeClient,
    ns: &str,
) -> Result<RegistryAuth> {
    let api: Api<Secret> = Api::namespaced(client, ns);
    let secret = match api.get_opt(secret_name).await? {
        Some(s) => s,
        None => return Ok(RegistryAuth::Anonymous),
    };
    let data = match secret.data {
        Some(d) => d,
        None => return Ok(RegistryAuth::Anonymous),
    };
    let raw = match data.get(".dockerconfigjson") {
        Some(b) => b.0.clone(),
        None => return Ok(RegistryAuth::Anonymous),
    };
    let config: serde_json::Value = serde_json::from_slice(&raw)?;
    let auth_b64 = config["auths"][registry]["auth"].as_str().unwrap_or("");
    if auth_b64.is_empty() {
        return Ok(RegistryAuth::Anonymous);
    }
    let decoded = String::from_utf8(
        base64::engine::general_purpose::STANDARD.decode(auth_b64)?,
    )?;
    let parts: Vec<&str> = decoded.splitn(2, ':').collect();
    if parts.len() == 2 {
        Ok(RegistryAuth::Basic(parts[0].to_string(), parts[1].to_string()))
    } else {
        Ok(RegistryAuth::Anonymous)
    }
}

/// Vérifie qu'un tag existe dans un registre OCI via une requête de manifest
/// (sans télécharger le contenu). Retourne false si le tag est absent (404),
/// true s'il est accessible, et propage les autres erreurs réseau.
pub async fn verify_tag_in_registry(
    registry: &str,
    image: &str,
    tag: &str,
    auth: RegistryAuth,
) -> Result<bool> {
    let oci = Client::new(client::ClientConfig::default());
    let reference = Reference::with_tag(registry.to_string(), image.to_string(), tag.to_string());
    match oci.pull_manifest_raw(&reference, &auth, &[]).await {
        Ok(_) => Ok(true),
        Err(oci_client::errors::OciDistributionError::RegistryError { envelope, .. })
            if envelope
                .errors
                .iter()
                .any(|e| {
                    matches!(
                        e.code,
                        oci_client::errors::OciErrorCode::ManifestUnknown
                            | oci_client::errors::OciErrorCode::NotFound
                    )
                }) =>
        {
            Ok(false)
        }
        Err(oci_client::errors::OciDistributionError::ServerError { code: 404, .. }) => Ok(false),
        Err(e) => Err(Error::OCIDistrib(e)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use http::{Request, Response, StatusCode};
    use kube::client::Body;
    use std::pin::pin;
    use tower_test::mock;

    fn make_secret_json(registry: &str, auth_b64: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": { "name": "my-secret", "namespace": "vynil-system" },
            "type": "kubernetes.io/dockerconfigjson",
            "data": {
                ".dockerconfigjson": base64::engine::general_purpose::STANDARD.encode(
                    serde_json::json!({
                        "auths": {
                            registry: { "auth": auth_b64 }
                        }
                    }).to_string()
                )
            }
        }))
        .unwrap()
    }

    fn make_404_json() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "kind": "Status",
            "apiVersion": "v1",
            "status": "Failure",
            "reason": "NotFound",
            "code": 404
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn test_resolve_registry_auth_valid_secret() {
        // "user:pass" en base64
        let auth_b64 = "dXNlcjpwYXNz";
        let body = make_secret_json("registry.example.com", auth_b64);

        let (mock_service, handle) = mock::pair::<Request<Body>, Response<Body>>();
        let spawned = tokio::spawn(async move {
            let mut handle = pin!(handle);
            let (_req, send) = handle.next_request().await.expect("service not called");
            send.send_response(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            );
        });

        let client = kube::Client::new(mock_service, "vynil-system");
        let auth = resolve_registry_auth("my-secret", "registry.example.com", client, "vynil-system")
            .await
            .unwrap();

        assert!(
            matches!(auth, RegistryAuth::Basic(ref u, ref p) if u == "user" && p == "pass"),
            "expected Basic(user, pass), got {auth:?}"
        );
        spawned.await.unwrap();
    }

    #[tokio::test]
    async fn test_resolve_registry_auth_secret_absent() {
        let body = make_404_json();

        let (mock_service, handle) = mock::pair::<Request<Body>, Response<Body>>();
        let spawned = tokio::spawn(async move {
            let mut handle = pin!(handle);
            let (_req, send) = handle.next_request().await.expect("service not called");
            send.send_response(
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            );
        });

        let client = kube::Client::new(mock_service, "vynil-system");
        let auth = resolve_registry_auth("my-secret", "registry.example.com", client, "vynil-system")
            .await
            .unwrap();

        assert!(matches!(auth, RegistryAuth::Anonymous));
        spawned.await.unwrap();
    }

    #[tokio::test]
    async fn test_resolve_registry_auth_registry_absent() {
        // Secret valide mais pour un autre registre
        let auth_b64 = "dXNlcjpwYXNz";
        let body = make_secret_json("other.registry.com", auth_b64);

        let (mock_service, handle) = mock::pair::<Request<Body>, Response<Body>>();
        let spawned = tokio::spawn(async move {
            let mut handle = pin!(handle);
            let (_req, send) = handle.next_request().await.expect("service not called");
            send.send_response(
                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            );
        });

        let client = kube::Client::new(mock_service, "vynil-system");
        let auth = resolve_registry_auth("my-secret", "registry.example.com", client, "vynil-system")
            .await
            .unwrap();

        assert!(matches!(auth, RegistryAuth::Anonymous));
        spawned.await.unwrap();
    }
}
