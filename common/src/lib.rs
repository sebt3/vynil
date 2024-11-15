use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("SerializationError: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("YamlError: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("K8s error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("K8s wait error: {0}")]
    KubeWaitError(#[from] kube::runtime::wait::Error),

    #[error("Elapsed wait error: {0}")]
    Elapsed(#[from] tokio::time::error::Elapsed),

    #[error("Finalizer error: {0}")]
    // NB: awkward type because finalizer::Error embeds the reconciler error (which is this)
    // so boxing this error to break cycles
    FinalizerError(#[from] Box<kube::runtime::finalizer::Error<Error>>),

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
    MissingScript(PathBuf),

    #[error("Missing destination directory {0}")]
    MissingDestination(PathBuf),

    #[error("UTF8 error {0}")]
    UTF8(#[from] std::string::FromUtf8Error),

    #[error("Semver error {0}")]
    Semver(#[from] semver::Error),

    #[error("Argon2 password_hash error {0}")]
    Argon2hash(#[from] argon2::password_hash::Error),

    #[error("Stdio error {0}")]
    Stdio(#[from] std::io::Error),

    #[error("OCI jukebox error {0}")]
    OCIDistrib(#[from] oci_client::errors::OciDistributionError),
    #[error("OCI parse error {0}")]
    OCIParseError(#[from] oci_client::ParseError),

    #[error("Base64 decode error {0}")]
    Base64DecodeError(#[from] base64::DecodeError),

    #[error("Error: {0}")]
    Other(String),
}
impl Error {
    pub fn metric_label(&self) -> String {
        format!("{self:?}").to_lowercase()
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
pub type RhaiRes<T> = std::result::Result<T, Box<rhai::EvalAltResult>>;
pub fn rhai_err(e: Error) -> Box<rhai::EvalAltResult> {
    format!("{e}").into()
}
pub mod context;
pub mod handlebarshandler;
pub mod hasheshandlers;
pub mod httphandler;
pub mod instancesystem;
pub mod instancetenant;
pub mod jukebox;
pub mod k8sgeneric;
pub mod k8sworkload;
pub mod ocihandler;
pub mod passwordhandler;
pub mod rhaihandler;
mod semverhandler;
pub mod shellhandler;
mod tools;
pub mod vynilpackage;
pub use context::get_client_name;
pub use semverhandler::Semver;
