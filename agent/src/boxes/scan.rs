use clap::Args;
use common::{
    Error, Result,
    context::set_box,
    jukebox::{JukeBox, JukeBoxDef},
    rhaihandler::Script,
};
use k8s_openapi::api::core::v1::Secret;
use kube::api::Api;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Args, Debug, Serialize, Deserialize)]
pub struct Parameters {
    /// Jukebox name to scan
    #[arg(short = 'j', long = "jukebox", env = "JUKEBOX", value_name = "JUKEBOX")]
    jukebox: String,
    /// Namespace to read secret from
    #[arg(
        short = 'v',
        long = "vynil-namespace",
        env = "VYNIL_NAMESPACE",
        value_name = "VYNIL_NAMESPACE"
    )]
    namespace: String,
    /// Agent script directory
    #[arg(
        short = 's',
        long = "script-dir",
        env = "SCRIPT_DIRECTORY",
        value_name = "SCRIPT_DIRECTORY",
        default_value = "./agent/scripts"
    )]
    script_dir: PathBuf,
    /// Filtre partiel : "<category>" ou "<category>/<package_name>"
    #[arg(short = 'f', long = "filter", env = "SCAN_PACKAGE", value_name = "SCAN_PACKAGE")]
    filter: Option<String>,
}

async fn resolve_http_secret(
    name: &str,
    namespace: &str,
    client: &kube::Client,
) -> Result<(String, String)> {
    let api: Api<Secret> = Api::namespaced(client.clone(), namespace);
    let secret = api.get(name).await.map_err(Error::KubeError)?;
    let data = secret.data.unwrap_or_default();
    if let Some(token_bytes) = data.get("token") {
        let token = String::from_utf8(token_bytes.0.clone()).map_err(Error::UTF8)?;
        return Ok(("bearer".to_string(), token));
    }
    let user = data
        .get("username")
        .and_then(|b| String::from_utf8(b.0.clone()).ok())
        .unwrap_or_default();
    let pass = data
        .get("password")
        .and_then(|b| String::from_utf8(b.0.clone()).ok())
        .unwrap_or_default();
    Ok(("basic".to_string(), format!("{}:{}", user, pass)))
}

pub async fn run(args: &Parameters) -> Result<()> {
    let mut rhai = Script::new(vec![
        format!("{}/boxes", args.script_dir.display()),
        format!("{}/lib", args.script_dir.display()),
    ]);
    let context = JukeBox::get(args.jukebox.clone()).await?;
    set_box(context.clone());
    rhai.ctx.set_value("box", context.clone());
    rhai.set_dynamic("args", &serde_json::to_value(args).unwrap());
    if let Some(JukeBoxDef::Http { secret, .. }) = &context.spec.source {
        if let Some(secret_name) = secret {
            let client = common::context::get_client_async().await;
            let (auth_type, credential) =
                resolve_http_secret(secret_name, &args.namespace, &client).await?;
            rhai.ctx.set_value("http_auth_type", auth_type);
            rhai.ctx.set_value("http_credential", credential);
        }
    }
    let _ = rhai.run_file(&PathBuf::from(format!(
        "{}/boxes/scan.rhai",
        args.script_dir.display()
    )))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{Request, Response, StatusCode};
    use kube::client::Body;
    use std::pin::pin;
    use tower_test::mock;

    fn b64(s: &str) -> String {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        STANDARD.encode(s)
    }

    fn make_secret_token(token: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": { "name": "my-secret", "namespace": "test-ns" },
            "type": "Opaque",
            "data": {
                "token": b64(token)
            }
        }))
        .unwrap()
    }

    fn make_secret_basic(username: &str, password: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": { "name": "my-secret", "namespace": "test-ns" },
            "type": "Opaque",
            "data": {
                "username": b64(username),
                "password": b64(password)
            }
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn test_resolve_http_secret_bearer() {
        let body = make_secret_token("my-bearer-token");

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

        let client = kube::Client::new(mock_service, "test-ns");
        let (auth_type, credential) = resolve_http_secret("my-secret", "test-ns", &client)
            .await
            .unwrap();

        assert_eq!(auth_type, "bearer");
        assert_eq!(credential, "my-bearer-token");
        spawned.await.unwrap();
    }

    #[tokio::test]
    async fn test_resolve_http_secret_basic() {
        let body = make_secret_basic("user", "pass");

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

        let client = kube::Client::new(mock_service, "test-ns");
        let (auth_type, credential) = resolve_http_secret("my-secret", "test-ns", &client)
            .await
            .unwrap();

        assert_eq!(auth_type, "basic");
        assert_eq!(credential, "user:pass");
        spawned.await.unwrap();
    }
}
