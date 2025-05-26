mod service;
mod system;
mod tenant;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::process;


#[derive(Parser, Debug)]
pub struct Parameters {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Template a tenant package
    Tenant(tenant::Parameters),
    /// Template a service package
    Service(service::Parameters),
    /// Template a system package
    System(system::Parameters),
}

pub async fn run(cmd: &Parameters) {
    match &cmd.command {
        Commands::Tenant(args) => tenant::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Templating a tenant package failed with: {e:}");
            process::exit(1)
        }),
        Commands::Service(args) => service::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Templating a service package failed with: {e:}");
            process::exit(1)
        }),
        Commands::System(args) => system::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Templating a system package failed with: {e:}");
            process::exit(3)
        }),
    }
}

#[derive(clap::ValueEnum, Default, Debug, Clone, Deserialize, Serialize)]
pub enum Contexts {
    /// Minimal context
    #[default]
    Simple,
    /// High-availibity context
    HA,
}
