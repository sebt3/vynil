use crate::{anonymize::scrub_yaml, dto::ScrubStats, error::DiagError};
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{Api, Client};

/// Get the vynil configuration from the `vynil` ConfigMap. Redaction stats are returned to the
/// caller (the handler emits them as the `X-Diag-Redactions` header, like every other endpoint).
pub async fn get_vynil_config(
    client: &Client,
    vynil_namespace: &str,
    namespace: &str,
) -> Result<(String, ScrubStats), DiagError> {
    let api: Api<ConfigMap> = Api::namespaced(client.clone(), vynil_namespace);

    match api.get("vynil").await {
        Ok(configmap) => {
            let data = configmap.data.unwrap_or_default();
            if data.is_empty() {
                return Ok(("---".to_string(), ScrubStats::default()));
            }
            let yaml = serde_yaml::to_string(&data).map_err(DiagError::YamlError)?;
            let (scrubbed, stats) = scrub_yaml(&yaml, client, namespace, vynil_namespace).await;
            Ok((scrubbed, stats))
        }
        // ConfigMap absent → empty document, not an error.
        Err(_) => Ok(("---".to_string(), ScrubStats::default())),
    }
}
