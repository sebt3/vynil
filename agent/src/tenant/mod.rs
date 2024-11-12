//mod backup;
mod delete;
mod install;
//mod reconfigure;
//mod restore;
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
    // Backup an instance
    //    Backup(backup::Parameters),
    // Restore an instance
    //    Restore(restore::Parameters),
    // Reconfigure an instance
    //    Reconfigure(reconfigure::Parameters),
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
        /*Commands::Backup(args) => backup::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Backup of a package failed with: {e:}");
            process::exit(4)
        }),
        Commands::Restore(args) => restore::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Restore of a package failed with: {e:}");
            process::exit(5)
        }),
        Commands::Reconfigure(args) => reconfigure::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Reconfiguring of a package failed with: {e:}");
            process::exit(6)
        }),*/
    }
}
