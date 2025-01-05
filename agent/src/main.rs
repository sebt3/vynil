mod boxes;
mod crdgen;
mod package;
mod run;
mod system;
mod tenant;
mod version;
use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Parameters {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run given git repo as a jukebox source
    Run(run::Parameters),
    /// generate CRDs
    Crdgen(crdgen::Parameters),
    /// package sub-command
    Package(package::Parameters),
    /// System instance sub-command
    System(system::Parameters),
    /// Tenant limited instance sub-command
    Tenant(tenant::Parameters),
    /// box sub-command
    Box(boxes::Parameters),
    /// Version sub-command
    Version(version::Parameters),
}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(
        env_logger::Env::default()
            .filter_or("LOG_LEVEL", "info")
            .write_style_or("LOG_STYLE", "auto"),
    );
    common::context::set_agent();
    common::context::init_k8s();
    let args = Parameters::parse();
    match &args.command {
        Commands::Version(args) => version::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Version failed with: {e:}");
            process::exit(1)
        }),
        Commands::Run(args) => run::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Run failed with: {e:}");
            process::exit(1)
        }),
        Commands::Crdgen(args) => crdgen::run(args).await.unwrap_or_else(|e| {
            tracing::error!("CRD generation failed with: {e:}");
            process::exit(2)
        }),
        Commands::Package(args) => package::run(args).await,
        Commands::System(args) => system::run(args).await,
        Commands::Tenant(args) => tenant::run(args).await,
        Commands::Box(args) => boxes::run(args).await,
    }
}
