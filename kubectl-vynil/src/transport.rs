use std::str::FromStr;

use anyhow::Context;
use http_body_util::BodyExt;
use kube::Client;

use crate::cli::InstanceTarget;

/// Result of fetching a single diagnostic item.
#[derive(Debug)]
pub struct GetResult {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: String,
    pub redactions: Option<(usize, usize)>,
}

/// Transport mode.
#[derive(Debug, Clone)]
pub enum TransportMode {
    /// Via kube apiserver aggregation (default, production).
    Aggregation,
    /// Direct HTTP call to the server (test/dev).
    Direct {
        server_url: String,
        token: String,
        insecure: bool,
    },
}

/// Fetches a single diagnostic item via the configured transport.
pub async fn get_item(mode: &TransportMode, target: &InstanceTarget, item: &str) -> GetResult {
    let path = crate::items::api_path(target, item);

    match mode {
        TransportMode::Aggregation => fetch_aggregation(&path).await,
        TransportMode::Direct {
            server_url,
            token,
            insecure,
        } => fetch_direct(server_url, &path, token, *insecure).await,
    }
}

/// Fetch via kube apiserver aggregation layer.
async fn fetch_aggregation(path: &str) -> GetResult {
    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: failed to create kube client: {}", e);
            return GetResult {
                status: 0,
                body: format!("DIAG-ERR: {}", e).into_bytes(),
                content_type: "text/plain".to_string(),
                redactions: None,
            };
        }
    };

    let req = match http::Request::get(path).body(kube::client::Body::empty()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("warning: failed to build request: {}", e);
            return GetResult {
                status: 0,
                body: format!("DIAG-ERR: {}", e).into_bytes(),
                content_type: "text/plain".to_string(),
                redactions: None,
            };
        }
    };

    match client.send(req).await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let content_type = resp
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let redactions = parse_redactions_header(resp.headers());
            let body = match BodyExt::collect(resp.into_body()).await {
                Ok(aggregated) => aggregated.to_bytes().to_vec(),
                Err(e) => {
                    eprintln!("warning: failed to read response body: {}", e);
                    Vec::new()
                }
            };
            GetResult {
                status,
                body,
                content_type,
                redactions,
            }
        }
        Err(e) => {
            eprintln!("warning: aggregation request failed: {}", e);
            GetResult {
                status: 0,
                body: format!("DIAG-ERR: {}", e).into_bytes(),
                content_type: "text/plain".to_string(),
                redactions: None,
            }
        }
    }
}

/// Fetch via direct HTTP call.
async fn fetch_direct(server_url: &str, path: &str, token: &str, insecure: bool) -> GetResult {
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);

    let mut builder = reqwest::ClientBuilder::new();
    if insecure {
        builder = builder.danger_accept_invalid_certs(true);
    }

    let client = match builder.build() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: failed to build HTTP client: {}", e);
            return GetResult {
                status: 0,
                body: format!("DIAG-ERR: {}", e).into_bytes(),
                content_type: "text/plain".to_string(),
                redactions: None,
            };
        }
    };

    match client
        .get(&url)
        .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let content_type = resp
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let redactions = parse_redactions_header(resp.headers());
            let body = match resp.bytes().await {
                Ok(bytes) => bytes.to_vec(),
                Err(e) => {
                    eprintln!("warning: failed to read response body: {}", e);
                    Vec::new()
                }
            };
            GetResult {
                status,
                body,
                content_type,
                redactions,
            }
        }
        Err(e) => {
            eprintln!("warning: direct request failed: {}", e);
            GetResult {
                status: 0,
                body: format!("DIAG-ERR: {}", e).into_bytes(),
                content_type: "text/plain".to_string(),
                redactions: None,
            }
        }
    }
}

/// Parses `X-Diag-Redactions: distinct=N;occurrences=M` header.
pub fn parse_redactions_header(headers: &http::HeaderMap) -> Option<(usize, usize)> {
    let header = headers.get("X-Diag-Redactions")?;
    let value = header.to_str().ok()?;
    parse_redactions(value)
}

/// Parses `distinct=N;occurrences=M` into `(N, M)`.
pub fn parse_redactions(value: &str) -> Option<(usize, usize)> {
    let mut distinct: Option<usize> = None;
    let mut occurrences: Option<usize> = None;

    for part in value.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("distinct=") {
            distinct = Some(usize::from_str(val).ok()?);
        } else if let Some(val) = part.strip_prefix("occurrences=") {
            occurrences = Some(usize::from_str(val).ok()?);
        }
    }

    match (distinct, occurrences) {
        (Some(d), Some(o)) => Some((d, o)),
        _ => None,
    }
}

/// Reads the in-cluster service account token.
pub fn read_sa_token() -> anyhow::Result<String> {
    let token_path = "/var/run/secrets/kubernetes.io/serviceaccount/token";
    std::fs::read_to_string(token_path).context("cannot read SA token")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_redactions_valid() {
        let result = parse_redactions("distinct=5;occurrences=12");
        assert_eq!(result, Some((5, 12)));
    }

    #[test]
    fn test_parse_redactions_reversed_order() {
        let result = parse_redactions("occurrences=20;distinct=3");
        assert_eq!(result, Some((3, 20)));
    }

    #[test]
    fn test_parse_redactions_with_spaces() {
        let result = parse_redactions("distinct = 7 ; occurrences = 15");
        assert_eq!(result, None); // prefix match is strict on "distinct="
    }

    #[test]
    fn test_parse_redactions_missing_distinct() {
        let result = parse_redactions("occurrences=10");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_redactions_missing_occurrences() {
        let result = parse_redactions("distinct=3");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_redactions_malformed() {
        assert_eq!(parse_redactions("garbage"), None);
        assert_eq!(parse_redactions(""), None);
        assert_eq!(parse_redactions("distinct=abc;occurrences=10"), None);
    }

    #[test]
    fn test_parse_redactions_header() {
        let mut headers = http::HeaderMap::new();
        headers.insert("X-Diag-Redactions", "distinct=4;occurrences=9".parse().unwrap());
        assert_eq!(parse_redactions_header(&headers), Some((4, 9)));

        let empty = http::HeaderMap::new();
        assert_eq!(parse_redactions_header(&empty), None);
    }
}
