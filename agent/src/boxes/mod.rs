mod scan;
use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser, Debug)]
pub struct Parameters {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Update a jukebox
    Scan(scan::Parameters),
}

pub async fn run(cmd: &Parameters) {
    common::context::init_k8s();
    match &cmd.command {
        Commands::Scan(args) => scan::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Scanning JukeBox failed with: {e:}");
            process::exit(1)
        }),
    }
}
