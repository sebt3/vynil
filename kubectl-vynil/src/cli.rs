use clap::Parser;
use serde::Serialize;

/// Vynil diagnostic CLI — collect artefacts and produce an auditable tar.gz bundle.
#[derive(Parser, Debug)]
#[command(name = "kubectl-vynil", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Parser, Debug)]
pub enum Commands {
    /// Collect diagnostic artefacts for a Vynil instance and produce a tar.gz bundle.
    Diagnose(DiagnoseArgs),
}

#[derive(Parser, Debug)]
pub struct DiagnoseArgs {
    /// Target instance as `<kind>/<name>`.
    /// `<kind>` ∈ { tenantinstances, serviceinstances, systeminstances }
    /// (aliases: ti, si, sysi).
    pub target: String,

    /// Namespace of the instance.
    #[arg(short, long)]
    pub namespace: String,

    /// Comma-separated subset of items to collect. Defaults to all.
    /// Known items: clusterinfo, vynilconfig, packages, state, children, agentlog, childlogs, operatorlog.
    #[arg(long, value_delimiter = ',')]
    pub items: Option<Vec<String>>,

    /// Output file path. Defaults to `<name>-diag-<timestamp>.tar.gz`.
    #[arg(short = 'o', long)]
    pub output: Option<String>,

    /// Direct mode: server URL to call directly (bypasses apiserver aggregation).
    /// Used for testing against a local server.
    #[arg(long)]
    pub server_url: Option<String>,

    /// Bearer token for direct mode. Falls back to in-cluster SA token if not provided.
    #[arg(long)]
    pub token: Option<String>,

    /// Direct mode: skip TLS certificate verification (dev only).
    #[arg(long, default_value_t = false)]
    pub insecure: bool,
}

/// Parsed instance target.
#[derive(Debug, Clone, Serialize)]
pub struct InstanceTarget {
    pub namespace: String,
    pub kind: String,
    pub name: String,
}

impl InstanceTarget {
    pub fn parse(target: &str, namespace: &str) -> Result<Self, String> {
        let parts: Vec<&str> = target.split('/').collect();
        if parts.len() != 2 {
            return Err(format!("invalid target '{}': expected <kind>/<name>", target));
        }
        let kind = normalize_kind(parts[0])?;
        let name = parts[1];
        if name.is_empty() {
            return Err("name must not be empty".to_string());
        }
        Ok(InstanceTarget {
            namespace: namespace.to_string(),
            kind,
            name: name.to_string(),
        })
    }
}

/// Normalizes kind alias to full plural resource name.
pub fn normalize_kind(kind: &str) -> Result<String, String> {
    match kind {
        "tenantinstances" | "ti" => Ok("tenantinstances".to_string()),
        "serviceinstances" | "si" => Ok("serviceinstances".to_string()),
        "systeminstances" | "sysi" => Ok("systeminstances".to_string()),
        _ => Err(format!(
            "unknown kind '{}': must be one of tenantinstances (ti), serviceinstances (si), systeminstances (sysi)",
            kind
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_kind_full() {
        assert_eq!(normalize_kind("tenantinstances").unwrap(), "tenantinstances");
        assert_eq!(normalize_kind("serviceinstances").unwrap(), "serviceinstances");
        assert_eq!(normalize_kind("systeminstances").unwrap(), "systeminstances");
    }

    #[test]
    fn test_normalize_kind_alias() {
        assert_eq!(normalize_kind("ti").unwrap(), "tenantinstances");
        assert_eq!(normalize_kind("si").unwrap(), "serviceinstances");
        assert_eq!(normalize_kind("sysi").unwrap(), "systeminstances");
    }

    #[test]
    fn test_normalize_kind_unknown() {
        assert!(normalize_kind("foo").is_err());
        assert!(normalize_kind("tenantinstance").is_err());
    }

    #[test]
    fn test_instance_target_parse_valid() {
        let target = InstanceTarget::parse("systeminstances/reloader", "kydah-core").unwrap();
        assert_eq!(target.kind, "systeminstances");
        assert_eq!(target.name, "reloader");
        assert_eq!(target.namespace, "kydah-core");
    }

    #[test]
    fn test_instance_target_parse_alias() {
        let target = InstanceTarget::parse("sysi/reloader", "kydah-core").unwrap();
        assert_eq!(target.kind, "systeminstances");
        assert_eq!(target.name, "reloader");
    }

    #[test]
    fn test_instance_target_parse_invalid() {
        assert!(InstanceTarget::parse("reloader", "ns").is_err());
        assert!(InstanceTarget::parse("kind/", "ns").is_err());
        assert!(InstanceTarget::parse("/name", "ns").is_err());
    }
}
