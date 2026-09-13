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

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("client", &"kube::Client")
            .field("config", &self.config)
            .finish()
    }
}

impl AppState {
    /// Create a new `AppState` from configuration
    ///
    /// # Errors
    ///
    /// Returns [`DiagError::InternalError`] when the Kubernetes client cannot be built from the
    /// in-cluster or kubeconfig environment.
    pub async fn new(config: &Config) -> Result<Self, crate::error::DiagError> {
        let client = Client::try_default()
            .await
            .map_err(|e| crate::error::internal_error(format!("Failed to create kube client: {e}")))?;

        Ok(Self {
            client,
            config: config.clone(),
        })
    }
}
