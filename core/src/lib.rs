use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("SerializationError: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("YamlError: {0}")]
    YamlError(String),

    #[error("Registering template failed with error: {0}")]
    HbsTemplateError(#[from] handlebars::TemplateError),

    #[error("Renderer error: {0}")]
    HbsRenderError(#[from] handlebars::RenderError),

    #[error("Rhai script error: {0}")]
    RhaiError(#[from] Box<rhai::EvalAltResult>),

    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("Json decoding error: {0}")]
    JsonError(#[source] serde_json::Error),

    #[error("{0} query failed: {1}")]
    MethodFailed(String, u16, String),

    #[error("Unsupported method")]
    UnsupportedMethod,

    #[error("Missing script {0}")]
    MissingScript(std::path::PathBuf),

    #[error("UTF8 error {0}")]
    UTF8(#[from] std::string::FromUtf8Error),

    #[error("Semver error {0}")]
    Semver(#[from] ::semver::Error),

    #[error("Argon2 password_hash error {0}")]
    Argon2hash(#[from] argon2::password_hash::Error),

    #[error("Bcrypt hash error {0}")]
    BcryptError(#[from] bcrypt::BcryptError),

    #[error("Stdio error {0}")]
    Stdio(#[from] std::io::Error),

    #[error("Base64 decode error {0}")]
    Base64DecodeError(#[from] base64::DecodeError),

    #[error("RAW api error {0}")]
    RawHTTP(#[from] ::http::Error),

    #[error("ParseIntError {0}")]
    ParseInt(#[from] std::num::ParseIntError),

    #[error("KEY-OPENSSL-001 OpenSSL error {0}")]
    OpenSSL(#[from] openssl::error::ErrorStack),

    #[error("KEY-ALGO-001 Unsupported key algorithm: {0}")]
    UnsupportedKeyAlgorithm(String),

    #[error("{0}")]
    PasswordSpec(String),

    #[error("Error: {0}")]
    Other(String),

    #[cfg(feature = "oci")]
    #[error("OCI jukebox error {0}")]
    OCIDistrib(#[from] oci_client::errors::OciDistributionError),

    #[cfg(feature = "oci")]
    #[error("OCI parse error {0}")]
    OCIParseError(#[from] oci_client::ParseError),

    #[cfg(feature = "k8s")]
    #[error("K8s error: {0}")]
    KubeError(#[from] kube::Error),

    #[cfg(feature = "k8s")]
    #[error("K8s wait error: {0}")]
    KubeWaitError(#[from] kube::runtime::wait::Error),

    #[cfg(feature = "k8s")]
    #[error("Elapsed wait error: {0}")]
    Elapsed(#[from] tokio::time::error::Elapsed),

    #[cfg(feature = "k8s")]
    #[error("Finalizer error: {0}")]
    FinalizerError(#[from] Box<kube::runtime::finalizer::Error<Error>>),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
pub type RhaiRes<T> = std::result::Result<T, Box<rhai::EvalAltResult>>;

pub fn rhai_err(e: Error) -> Box<rhai::EvalAltResult> {
    e.to_string().into()
}

pub fn rhai_err_str(e: String) -> Box<rhai::EvalAltResult> {
    e.into()
}

pub mod chrono;
pub mod glob;
pub mod hashes;
pub mod http;
pub mod http_mock;
pub mod key;
pub mod password;
pub mod semver;
pub mod shell;

#[cfg(feature = "oci")] pub mod oci;

#[cfg(feature = "s3")] pub mod s3;

#[cfg(feature = "k8s")] pub mod k8s;

pub use semver::Semver;

#[cfg(feature = "k8s")] pub use k8s::update_cache;
