pub mod install;
pub mod distrib;
pub mod events;
pub mod handlers;
pub mod yaml;
mod clusterissuers;
mod tenants;
pub use anyhow::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub use kube::Client;
pub async fn get_client() -> Client {
    Client::try_default().await.expect("create client")
}
