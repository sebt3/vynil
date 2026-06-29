use clap::Parser;
use std::path::PathBuf;

/// Configuration for the vynil-diag diagnostic server
#[derive(Parser, Debug, Clone)]
#[command(name = "vynil-diag")]
#[command(author, version, about, long_about = None)]
pub struct Config {
    /// Address to bind the server to
    #[arg(long, env = "DIAG_BIND", default_value = "0.0.0.0:8443")]
    pub bind: String,

    /// Enable insecure HTTP (no TLS)
    #[arg(long, env = "INSECURE_NO_TLS", default_value = "false")]
    pub insecure_no_tls: bool,

    /// TLS certificate file path
    #[arg(long, env = "TLS_CERT")]
    pub tls_cert: Option<PathBuf>,

    /// TLS key file path
    #[arg(long, env = "TLS_KEY")]
    pub tls_key: Option<PathBuf>,

    /// Optional override for the front-proxy CA (PEM file) used to verify the apiserver client
    /// cert. By default the server self-loads it from the extension-apiserver-authentication
    /// ConfigMap in kube-system, so this is only an escape hatch / offline use.
    #[arg(long, env = "REQUESTHEADER_CLIENT_CA")]
    pub requestheader_ca: Option<PathBuf>,

    /// Trust front-proxy identity headers (X-Remote-User/Group).
    /// SECURITY: only safe behind the aggregation layer. Enabling it forces mandatory mTLS
    /// verification of the apiserver client-cert (requires TLS + --requestheader-client-ca).
    /// When off (default), these headers are ignored and only Bearer/TokenReview auth is used.
    #[arg(long, env = "DIAG_TRUST_REQUEST_HEADER", default_value = "false")]
    pub trust_request_header: bool,

    /// Namespace where vynil controller is deployed
    #[arg(long, env = "VYNIL_NAMESPACE", default_value = "vynil-system")]
    pub vynil_namespace: String,

    /// Time window in hours for child logs collection
    #[arg(long, env = "LOG_SINCE_HOURS", default_value = "5")]
    pub log_since_hours: u64,

    /// Maximum bytes per container for log collection
    #[arg(long, env = "LOG_CAP_BYTES", default_value = "2097152")]
    pub log_cap_bytes: usize,

    /// Enable the packages endpoint
    #[arg(long, env = "DIAG_ENABLE_PACKAGES", default_value = "true")]
    pub enable_packages: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self::parse()
    }
}
