mod clone;
mod install;
mod template;
mod plan;
mod destroy;

use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Parameters {
    #[command(subcommand)]
    pub command: Commands
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Clone given git repo as a distribution source
    Clone(clone::Parameters),
    /// Template the application dist files to kustomize compatible files
    Template(template::Parameters),
    /// Plan the install
    Plan(plan::Parameters),
    /// Install given component
    Install(install::Parameters),
    /// Destroy given component
    Destroy(destroy::Parameters),
}


#[tokio::main]
async fn main() {
    // TODO: Support importing resources
    // TODO: Support auto-import of existing resources
    env_logger::init_from_env(env_logger::Env::default().filter_or("LOG_LEVEL", "info").write_style_or("LOG_STYLE", "auto"));
    let args = Parameters::parse();
    match &args.command {
        Commands::Clone(args)   => {match clone::run(args).await {
            Ok(d) => d, Err(e) => {
                log::error!("Clone failed with: {e:}");
                process::exit(1)
            }
        }}
        // install init:1
            Commands::Template(args) => {match template::run(args).await {
            Ok(d) => d, Err(e) => {
                log::error!("Template failed with: {e:}");
                process::exit(1)
            }
        }},
        // install init:2 (or plan container, with the same previous init stage)
        Commands::Plan(args) => {match plan::run(args).await {
            Ok(d) => d, Err(e) => {
                log::error!("Plan failed with: {e:}");
                process::exit(1)
            }
        }},
        // install container
        Commands::Install(args) => {match install::run(args).await {
            Ok(d) => d, Err(e) => {
                log::error!("Install failed with: {e:}");
                process::exit(1)
            }
        }},
        // destroy container
        Commands::Destroy(args) => {match destroy::run(args).await {
            Ok(d) => d, Err(e) => {
                log::error!("Install failed with: {e:}");
                process::exit(1)
            }
        }},
    }
}
