use async_trait::async_trait;
use std::sync::Arc;
use manager::Context;
use kube::runtime::controller::Action;
pub mod jukebox;
pub mod instancesystem;
pub mod instancetenant;

pub use common::Error;
pub use common::Result;
pub use common::jukebox::JukeBox;
pub use common::instancesystem::SystemInstance;
pub use common::instancetenant::TenantInstance;

#[async_trait]
pub trait Reconciler {
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action>;
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action>;
}

pub static OPERATOR: &str = "operator.vynil.solidite.fr";

/// State machinery for kube, as exposeable to actix
pub mod manager;
pub use manager::Manager;

/// Log and trace integrations
pub mod telemetry;

/// Metrics
mod metrics;
pub use metrics::Metrics;
