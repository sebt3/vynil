use std::{process, path::{PathBuf, Path}, fs};
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use package::{terraform, yaml, script};

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
    /// Generate index.yaml (when creating a new project)
    Index(ParametersDest),
    /// Generate index.rhai
    Rhai(ParametersDest),
    /// Generate index.yaml options based on the default values
    Options(ParametersDest),
}

fn providers(args:&ParametersDest) -> Result<()> {
    if ! Path::new(&args.project).is_dir() {
        bail!("{:?} is not a directory", args.project);
    }
    let mut file = PathBuf::new();
    file.push(args.project.clone());
    file.push("index.yaml");
    let yaml = match yaml::read_index(&file){Ok(d) => d, Err(e) => {
        bail!("{e}");
    }};
    terraform::gen_providers(&args.project, yaml.providers)
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
fn index(args:&ParametersDest) -> Result<()> {
    if ! Path::new(&args.project).is_dir() {
        bail!("{:?} is not a directory", args.project);
    }
    yaml::gen_index(&args.project)
}
fn rhai(args:&ParametersDest) -> Result<()> {
    if ! Path::new(&args.project).is_dir() {
        bail!("{:?} is not a directory", args.project);
    }
    script::gen_index(&args.project)
}
fn options(args:&ParametersDest) -> Result<()> {
    let mut file = PathBuf::new();
    file.push(fs::canonicalize(&args.project).unwrap().as_os_str());
    file.push("index.yaml");
    let yaml = match yaml::read_index(&file) {Ok(d) => d, Err(e) => {log::error!("{e:}");process::exit(1)},};
    yaml.update_options_from_defaults(file)
}


pub fn run(args:&Parameters) {
    match &args.command {
        Commands::Providers(args) => {match providers(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Generating the providers.tf file failed with: {e:}");
                process::exit(1)
            }
        }}
        Commands::Ressources(args) => {match ressources(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Generating the ressources.tf file failed with: {e:}");
                process::exit(1)
            }
        }}
        Commands::Datas(args) => {match datas(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Generating the datas.tf file failed with: {e:}");
                process::exit(1)
            }
        }}
        Commands::Index(args) => {match index(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Generating the index.yaml file failed with: {e:}");
                process::exit(1)
            }
        }}
        Commands::Rhai(args) => {match rhai(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Generating the index.rhai file failed with: {e:}");
                process::exit(1)
            }
        }}
        Commands::Options(args) => {match options(args) {
            Ok(d) => d, Err(e) => {
                log::error!("Generation the options in the index.yaml failed with: {e:}");
                process::exit(1)
            }
        }}
    }
}
