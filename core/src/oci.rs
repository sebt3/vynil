use crate::{Error, Result, RhaiRes, rhai_err};
use base64::Engine as _;
use chrono::Utc;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use k8s_openapi::api::core::v1::Secret;
use kube::{Client as KubeClient, api::Api};
pub use oci_client::secrets::RegistryAuth as OciRegistryAuth;
use oci_client::{Client, Reference, client, config, manifest, secrets::RegistryAuth};
use rhai::{Dynamic, Engine, ImmutableString, Map};
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
    ) -> RhaiRes<ImmutableString> {
        let client = Client::new(client::ClientConfig::default());
        let reference = Reference::with_tag(self.registry.clone(), repository, tag);
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        for (key, val) in annotations {
            values.insert(key.into(), val.to_string());
        }
        let mut tar_uncompressed = Builder::new(Vec::new());
        tar_uncompressed
            .append_dir_all(".", source_dir)
            .map_err(|e| rhai_err(Error::Stdio(e)))?;
        let raw_tar = tar_uncompressed
            .into_inner()
            .map_err(|e| rhai_err(Error::Stdio(e)))?;
        let diff_id = format!("sha256:{}", sha256::digest(raw_tar.as_slice()));
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
        let push_response = block_in_place(|| {
            Handle::current().block_on(async move {
                client
                    .push(&reference, &layers, config, &self.auth.clone(), Some(manifest))
                    .await
            })
        })
        .map_err(|e| rhai_err(Error::OCIDistrib(e)))?;
        let manifest_url = push_response.manifest_url;
        let digest: ImmutableString = if let Some(idx) = manifest_url.rfind("sha256:") {
            manifest_url[idx..].to_string()
        } else {
            manifest_url
        }
        .into();
        Ok(digest)
    }

    pub fn sign_image(
        &mut self,
        repository: String,
        tag: String,
        digest: String,
        key_path: String,
    ) -> RhaiRes<()> {
        if key_path.is_empty() {
            return Ok(());
        }
        let image_ref = format!("{}/{}:{}@{}", self.registry, repository, tag, digest);
        let status = std::process::Command::new("cosign")
            .args(["sign", "--yes", "--key", &key_path, &image_ref])
            .status()
            .map_err(|e| rhai_err(Error::Stdio(e)))?;
        if status.success() {
            Ok(())
        } else {
            Err(rhai_err(Error::Other(format!(
                "cosign sign failed for {image_ref}"
            ))))
        }
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
    let decoded = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(auth_b64)?)?;
    let parts: Vec<&str> = decoded.splitn(2, ':').collect();
    if parts.len() == 2 {
        Ok(RegistryAuth::Basic(parts[0].to_string(), parts[1].to_string()))
    } else {
        Ok(RegistryAuth::Anonymous)
    }
}

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
            if envelope.errors.iter().any(|e| {
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

pub fn get_auth_from_file(path: String, registry: String) -> RhaiRes<Dynamic> {
    let content = std::fs::read_to_string(&path).map_err(|e| rhai_err(Error::Stdio(e)))?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| rhai_err(Error::SerializationError(e)))?;
    let auth_b64 = json["auths"][&registry]["auth"].as_str().unwrap_or_default();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(auth_b64)
        .unwrap_or_default();
    let user_pass = String::from_utf8(decoded).unwrap_or_default();
    let mut parts = user_pass.splitn(2, ':');
    let user = parts.next().unwrap_or_default().to_string();
    let pass = parts.next().unwrap_or_default().to_string();
    let mut map = Map::new();
    map.insert("user".into(), user.into());
    map.insert("pass".into(), pass.into());
    Ok(Dynamic::from_map(map))
}

pub fn oci_rhai_register(engine: &mut Engine) {
    engine
        .register_type_with_name::<Registry>("Registry")
        .register_fn("new_registry", Registry::new)
        .register_fn("push_image", Registry::push_image)
        .register_fn("sign_image", Registry::sign_image)
        .register_fn("list_tags", Registry::rhai_list_tags)
        .register_fn("get_manifest", Registry::get_manifest)
        .register_fn("get_auth_from_file", get_auth_from_file);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_docker_config(path: &std::path::Path, registry: &str, auth_b64: &str) {
        let content = serde_json::json!({
            "auths": { registry: { "auth": auth_b64 } }
        })
        .to_string();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn get_auth_from_file_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        write_docker_config(&path, "docker.io", "dXNlcjpwYXNz");
        let result = get_auth_from_file(path.to_string_lossy().to_string(), "docker.io".to_string());
        let map = result.unwrap().cast::<Map>();
        assert_eq!(map["user"].clone().cast::<String>(), "user");
        assert_eq!(map["pass"].clone().cast::<String>(), "pass");
    }

    #[test]
    fn get_auth_from_file_registry_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        write_docker_config(&path, "docker.io", "dXNlcjpwYXNz");
        let result = get_auth_from_file(path.to_string_lossy().to_string(), "ghcr.io".to_string());
        let map = result.unwrap().cast::<Map>();
        assert_eq!(map["user"].clone().cast::<String>(), "");
        assert_eq!(map["pass"].clone().cast::<String>(), "");
    }

    #[test]
    fn get_auth_from_file_not_found() {
        let result = get_auth_from_file(
            "/nonexistent/path/config.json".to_string(),
            "docker.io".to_string(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn get_auth_from_file_empty_auth() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        write_docker_config(&path, "docker.io", "");
        let result = get_auth_from_file(path.to_string_lossy().to_string(), "docker.io".to_string());
        let map = result.unwrap().cast::<Map>();
        assert_eq!(map["user"].clone().cast::<String>(), "");
        assert_eq!(map["pass"].clone().cast::<String>(), "");
    }

    #[test]
    fn sign_image_empty_key_returns_ok() {
        let mut reg = Registry::new("r.io".into(), "u".into(), "p".into());
        let result = reg.sign_image("repo/img".into(), "1.0.0".into(), "sha256:abc".into(), "".into());
        assert!(result.is_ok(), "Empty key must be a no-op");
    }

    #[test]
    fn sign_image_cosign_not_found_returns_error() {
        let mut reg = Registry::new("r.io".into(), "u".into(), "p".into());
        let result = reg.sign_image(
            "repo/img".into(),
            "1.0.0".into(),
            "sha256:abc".into(),
            "/nonexistent/key.pem".into(),
        );
        assert!(result.is_err(), "Non-existent key must produce an error");
    }

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
