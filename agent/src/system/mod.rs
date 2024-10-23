mod delete;
mod install;
use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser, Debug)]
pub struct Parameters {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Install an instance
    Install(install::Parameters),
    /// Delete an instance
    Delete(delete::Parameters),
}

pub async fn run(cmd: &Parameters) {
    match &cmd.command {
        Commands::Install(args) => install::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Installing a package failed with: {e:}");
            process::exit(1)
        }),
        Commands::Delete(args) => delete::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Deleting a package failed with: {e:}");
            process::exit(3)
        }),
    }
}
