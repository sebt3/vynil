use clap::Parser;

use kubectl_vynil::{
    actions::{run_instance, run_jukebox},
    cli::{Cli, Commands, SERVICE_INSTANCE, SYSTEM_INSTANCE, TENANT_INSTANCE},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider().install_default().ok();

    let cli = Cli::parse();

    match cli.command {
        Commands::Jukebox(args) => run_jukebox(&args).await,
        Commands::Vti(args) => run_instance(&TENANT_INSTANCE, &args).await,
        Commands::Vsvc(args) => run_instance(&SERVICE_INSTANCE, &args).await,
        Commands::Vsi(args) => run_instance(&SYSTEM_INSTANCE, &args).await,
    }
}
