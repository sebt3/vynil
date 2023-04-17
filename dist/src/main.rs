mod gen;
mod pack;
mod validate;

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
    /// Generate some terraform files
    Generate(gen::Parameters),
}

fn main() {
    //TODO: Support generating "options schema" from default values
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
        Commands::Generate(args) => {gen::run(args)}
    }
}
