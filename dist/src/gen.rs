use std::{process, path::{PathBuf, Path}};
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use package::terraform;

#[derive(Args, Debug)]
pub struct ParametersDest {
    /// Project source directory
    #[arg(short, long, value_name = "SOURCE_DIR", default_value = ".")]
    project: PathBuf,
}

#[derive(Args, Debug)]
#[command(propagate_version = true)]
pub struct Parameters {
    #[command(subcommand)]
    pub command: Commands
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Generate providers.tf
    Providers(ParametersDest),
    /// Generate ressources.tf
    Ressources(ParametersDest),
    /// Generate datas.tf
    Datas(ParametersDest),
}

fn providers(args:&ParametersDest) -> Result<()> {
    if ! Path::new(&args.project).is_dir() {
        bail!("{:?} is not a directory", args.project);
    }
    terraform::gen_providers(&args.project)
}
fn ressources(args:&ParametersDest) -> Result<()> {
    if ! Path::new(&args.project).is_dir() {
        bail!("{:?} is not a directory", args.project);
    }
    terraform::gen_ressources(&args.project)
}
fn datas(args:&ParametersDest) -> Result<()> {
    if ! Path::new(&args.project).is_dir() {
        bail!("{:?} is not a directory", args.project);
    }
    terraform::gen_datas(&args.project)
}


pub fn run(args:&Parameters) {
    match &args.command {
        Commands::Providers(args) => {match providers(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Validation failed with: {e:}");
                process::exit(1)
            }
        }}
        Commands::Ressources(args) => {match ressources(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Validation failed with: {e:}");
                process::exit(1)
            }
        }}
        Commands::Datas(args) => {match datas(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Validation failed with: {e:}");
                process::exit(1)
            }
        }}
    }
}
