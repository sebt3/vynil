use thiserror::Error;
use async_trait::async_trait;
use std::sync::Arc;
use manager::Context;
use kube::runtime::controller::Action;

#[derive(Error, Debug)]
pub enum Error {
    #[error("SerializationError: {0}")]
    SerializationError(#[source] serde_json::Error),

    #[error("Kube Error: {0}")]
    KubeError(#[source] kube::Error),

    #[error("Kube wait job Error: {0}")]
    WaitError(#[source] tokio::time::error::Elapsed),
    #[error("Kube job Error: {0}")]
    JobError(#[source] kube::runtime::wait::Error),

    #[error("Finalizer Error: {0}")]
    // NB: awkward type because finalizer::Error embeds the reconciler error (which is this)
    // so boxing this error to break cycles
    FinalizerError(#[source] Box<kube::runtime::finalizer::Error<Error>>),

    #[error("IllegalDistrib")]
    IllegalDistrib,

    #[error("IllegalInstall")]
    IllegalInstall,

    #[error("TooLongDelete")]
    TooLongDelete,
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    #[must_use] pub fn metric_label(&self) -> String {
        format!("{:?}", self).to_lowercase()
    }
}

#[async_trait]
pub trait Reconciler {
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action>;
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action>;
}

pub static OPERATOR: &str = "operator.vynil.solidite.fr";
pub static AGENT_IMAGE: &str  = "docker.io/sebt3/vynil-agent:0.1.1";

pub mod distrib;
pub mod install;
pub mod jobs;
pub mod cronjobs;
pub mod pvc;
pub mod events;

/// State machinery for kube, as exposeable to actix
pub mod manager;
pub use manager::Manager;

/// Generated type, for crdgen
pub use distrib::Distrib;
pub use install::Install;

/// Log and trace integrations
pub mod telemetry;

/// Metrics
mod metrics;
pub use metrics::Metrics;

//mod jobs;
