use async_trait::async_trait;
use kube::runtime::controller::Action;
use manager::Context;
use std::sync::Arc;
pub mod instanceservice;
pub mod instancesystem;
pub mod instancetenant;
pub mod jukebox;

pub use common::{
    Error, Result, instanceservice::ServiceInstance, instancesystem::SystemInstance,
    instancetenant::TenantInstance, jukebox::JukeBox,
};

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
pub fn get_client_name() -> String {
    "controller.vynil.solidite.fr".to_string()
}
