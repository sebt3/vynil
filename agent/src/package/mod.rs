mod build;
mod test;
mod unpack;
mod update;
use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser, Debug)]
pub struct Parameters {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Pack a directory into a package
    Build(build::Parameters),
    /// Update a package
    Update(update::Parameters),
    /// Test a package
    Test(test::Parameters),
    /// Unpack a directory into a package
    Unpack(unpack::Parameters),
}

pub async fn run(cmd: &Parameters) {
    match &cmd.command {
        Commands::Build(args) => build::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Packing directory failed with: {e:}");
            process::exit(1)
        }),
        Commands::Update(args) => update::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Updating the package failed with: {e:}");
            process::exit(2)
        }),
        Commands::Test(args) => test::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Testing the package failed with: {e:}");
            process::exit(3)
        }),
        Commands::Unpack(args) => unpack::run(args).await.unwrap_or_else(|e| {
            tracing::error!("Unpacking OCI image to directory failed with: {e:}");
            process::exit(4)
        }),
    }
}
