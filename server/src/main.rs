use clap::Parser;
use server::{config::Config, server::run_server, state::AppState};
use tracing::{error, info};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("server=debug,info")
        .init();

    // Both `ring` and `aws-lc-rs` exist in the dependency tree, so rustls cannot auto-pick a
    // provider — kube uses rustls to reach the apiserver and would panic. Pin it to ring.
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
    {
        tracing::debug!("rustls crypto provider already installed");
    }

    let config = Config::parse();

    info!("Starting vynil-diag server with config: {:?}", config);

    match AppState::new(&config).await {
        Ok(state) => {
            if let Err(e) = run_server(state, config).await {
                error!("Server error: {}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            error!("Failed to create app state: {}", e);
            std::process::exit(1);
        }
    }
}
