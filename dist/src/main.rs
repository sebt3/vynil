mod gen;
mod pack;
mod validate;
mod template;
mod plan;
mod check;
mod files;

use clap::{Parser, Subcommand};
use std::process;

/// Vynil: dist-tools
/// Vynil is kubernetes based cloud distribution
/// dist is a packaging tool for Vynil
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Parameters {
    #[command(subcommand)]
    pub command: Commands
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Validate given project
    Validate(validate::Parameters),
    /// Pack given project
    Pack(pack::Parameters),
    /// Run the check script
    Check(check::Parameters),
    /// Generate some terraform files
    Generate(gen::Parameters),
    /// Template the application dist files to kustomize compatible files
    Template(template::Parameters),
    /// Plan the install
    Plan(plan::Parameters),
}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::default().filter_or("LOG_LEVEL", "info").write_style_or("LOG_STYLE", "auto"));
    let args = Parameters::parse();
    match &args.command {
        Commands::Validate(args) => {match validate::run(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Validation failed with: {e:}");
                process::exit(1)
            }
        }}
        Commands::Pack(args) => {match pack::run(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Packing failed with: {e:}");
                process::exit(1)
            }
        }}
        Commands::Check(args) => {match check::run(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Check failed with: {e:}");
                process::exit(1)
            }
        }}
        Commands::Generate(args) => {gen::run(args)}
        Commands::Template(args) => {match template::run(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Template failed with: {e:}");
                process::exit(1)
            }
        }},
        Commands::Plan(args) => {match plan::run(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Plan failed with: {e:}");
                process::exit(1)
            }
        }},
   }
}
