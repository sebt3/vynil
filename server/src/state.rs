use crate::config::Config;
use kube::Client;

/// Application state shared across all requests
#[derive(Clone)]
pub struct AppState {
    /// Kubernetes client
    pub client: Client,
    /// Configuration
    pub config: Config,
}

impl AppState {
    /// Create a new AppState from configuration
    pub async fn new(config: &Config) -> Result<Self, crate::error::DiagError> {
        let client = Client::try_default()
            .await
            .map_err(|e| crate::error::internal_error(format!("Failed to create kube client: {}", e)))?;

        Ok(Self {
            client,
            config: config.clone(),
        })
    }
}
