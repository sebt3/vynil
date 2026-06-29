use base64::Engine as _;
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client};
use regex::Regex;
use std::collections::HashSet;

use crate::dto::ScrubStats;

const ANONYMIZED_TEXT: &str = "<anonymized>";

/// Minimum length for a secret value to be scrubbed.
/// Below this, substring replacement over-redacts structural content
/// (namespace names, labels, booleans...). Real k8s secrets/tokens are longer.
const MIN_SECRET_LEN: usize = 8;

/// Common values that, even if stored in a Secret, must never be used as a
/// scrubbing pattern: they appear everywhere in structural content.
const COMMON_DENYLIST: &[&str] = &[
    "true",
    "false",
    "default",
    "vynil",
    "system",
    "kubernetes",
    "cluster",
    "enabled",
    "disabled",
    "password",
    "username",
];

/// Scrub a text by replacing secret values and well-known secret patterns.
///
/// 1. Collects secret values from Secrets in the instance namespace + vynil namespace.
/// 2. Replaces every occurrence of those values with `<anonymized>`.
/// 3. Applies targeted pattern redaction (JWT, Bearer, PEM private keys).
///
/// Note: no blanket "long base64/hex" rule — it destroys digests, UIDs, hashes,
/// which are exactly what a maintainer needs (and breaks auditability).
pub async fn scrub(
    text: &str,
    client: &Client,
    namespace: &str,
    vynil_namespace: &str,
) -> (String, ScrubStats) {
    let secret_values = collect_secret_values(client, namespace, vynil_namespace).await;
    let (mut result, stats) = scrub_secrets(text, &secret_values);
    result = apply_patterns(&result);
    (result, stats)
}

/// Collect all non-trivial secret values from Secrets in the given namespaces.
async fn collect_secret_values(client: &Client, namespace: &str, vynil_namespace: &str) -> Vec<String> {
    let mut values = HashSet::new();

    if let Ok(secrets) = list_secrets(client, namespace).await {
        for secret in secrets {
            extract_secret_values(&secret, &mut values);
        }
    }
    if namespace != vynil_namespace
        && let Ok(secrets) = list_secrets(client, vynil_namespace).await
    {
        for secret in secrets {
            extract_secret_values(&secret, &mut values);
        }
    }

    values.into_iter().filter(|v| is_scrubbable(v)).collect()
}

/// A value is worth scrubbing if it is long enough and not a common token.
fn is_scrubbable(value: &str) -> bool {
    value.len() >= MIN_SECRET_LEN && !COMMON_DENYLIST.contains(&value.to_ascii_lowercase().as_str())
}

async fn list_secrets(client: &Client, namespace: &str) -> Result<Vec<Secret>, kube::Error> {
    let api: Api<Secret> = Api::namespaced(client.clone(), namespace);
    Ok(api.list(&Default::default()).await?.items)
}

/// Extract decoded values from a Secret's `data` and `stringData`.
fn extract_secret_values(secret: &Secret, values: &mut HashSet<String>) {
    if let Some(data) = &secret.data {
        for value in data.values() {
            // ByteString is already the decoded bytes.
            if let Ok(text) = String::from_utf8(value.0.clone()) {
                values.insert(text);
            }
        }
    }
    if let Some(string_data) = &secret.string_data {
        for value in string_data.values() {
            values.insert(value.clone());
        }
    }
    // Defensive: some serializations keep data base64-encoded as strings.
    if let Ok(json) = serde_json::to_value(secret)
        && let Some(data) = json.get("data").and_then(|d| d.as_object())
    {
        for value in data.values() {
            if let Some(s) = value.as_str()
                && let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(s)
                && let Ok(text) = String::from_utf8(decoded)
            {
                values.insert(text);
            }
        }
    }
}

/// Replace every occurrence of each secret value with `<anonymized>`.
/// Single-pass `str::replace` (no re-scan of the replacement → no infinite loop).
fn scrub_secrets(text: &str, secrets: &[String]) -> (String, ScrubStats) {
    let mut result = text.to_string();
    let mut distinct = 0;
    let mut occurrences = 0;

    for secret in secrets {
        if secret.is_empty() {
            continue;
        }
        let count = result.matches(secret.as_str()).count();
        if count > 0 {
            result = result.replace(secret.as_str(), ANONYMIZED_TEXT);
            distinct += 1;
            occurrences += count;
        }
    }

    (result, ScrubStats {
        distinct,
        occurrences,
    })
}

/// Targeted pattern redaction for well-known secret shapes.
fn apply_patterns(text: &str) -> String {
    let jwt = Regex::new(r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+").unwrap();
    let bearer = Regex::new(r"(?i)Bearer\s+[A-Za-z0-9._~+/=-]+").unwrap();
    let pem =
        Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----").unwrap();

    let mut result = jwt.replace_all(text, ANONYMIZED_TEXT).to_string();
    result = bearer.replace_all(&result, ANONYMIZED_TEXT).to_string();
    result = pem.replace_all(&result, ANONYMIZED_TEXT).to_string();
    result
}

/// Scrub a `serde_json::Value` via its string form.
pub async fn scrub_json(
    value: serde_json::Value,
    client: &Client,
    namespace: &str,
    vynil_namespace: &str,
) -> Result<(serde_json::Value, ScrubStats), serde_json::Error> {
    let text = serde_json::to_string(&value)?;
    let (scrubbed, stats) = scrub(&text, client, namespace, vynil_namespace).await;
    let scrubbed_value = serde_json::from_str(&scrubbed)?;
    Ok((scrubbed_value, stats))
}

/// Scrub a YAML string.
pub async fn scrub_yaml(
    yaml: &str,
    client: &Client,
    namespace: &str,
    vynil_namespace: &str,
) -> (String, ScrubStats) {
    scrub(yaml, client, namespace, vynil_namespace).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_secret_values_and_counts() {
        let text = "conn: postgres://user:Sup3rL0ngP4ss@db, again Sup3rL0ngP4ss";
        let secrets = vec!["Sup3rL0ngP4ss".to_string()];
        let (result, stats) = scrub_secrets(text, &secrets);
        assert!(!result.contains("Sup3rL0ngP4ss"));
        assert_eq!(stats.distinct, 1);
        assert_eq!(stats.occurrences, 2);
    }

    #[test]
    fn secret_substring_of_replacement_does_not_loop() {
        // "anon" is a substring of "<anonymized>": the old while-loop hung here.
        let text = "value anonymized-data";
        let secrets = vec!["anonymized".to_string()];
        let (result, _) = scrub_secrets(text, &secrets);
        assert!(result.contains(ANONYMIZED_TEXT));
    }

    #[test]
    fn does_not_redact_digests_or_long_hashes() {
        let text = "image: app@sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        // No secret values, only pattern pass.
        let result = apply_patterns(text);
        assert_eq!(result, text, "sha256 digests must survive anonymisation");
    }

    #[test]
    fn short_or_common_values_are_not_scrubbable() {
        assert!(!is_scrubbable("true"));
        assert!(!is_scrubbable("admin")); // too short
        assert!(!is_scrubbable("vynil")); // denylisted
        assert!(is_scrubbable("aV3ryL0ngSecretValue"));
    }

    #[test]
    fn redacts_jwt_and_pem() {
        let jwt = "tok eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.dozjgNryP4J3jVmNHl0w5N_XgL0";
        assert!(apply_patterns(jwt).contains(ANONYMIZED_TEXT));
        assert!(!apply_patterns(jwt).contains("eyJ"));

        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIabc\n-----END RSA PRIVATE KEY-----";
        assert!(apply_patterns(pem).contains(ANONYMIZED_TEXT));
        assert!(!apply_patterns(pem).contains("MIIabc"));
    }
}
