use std::{fs, process, path::{PathBuf, Path}};
use anyhow::{Result, bail};
use clap::Args;
use package::yaml;

#[derive(Args, Debug)]
pub struct Parameters {
    /// Project source directory
    #[arg(short, long, value_name = "SOURCE_DIR", default_value = "/work")]
    project: PathBuf,
}

pub fn run(args:&Parameters) -> Result<()> {
    // Validate that the project parameter is a directory
    if ! Path::new(&args.project).is_dir() {
        bail!("{:?} is not a directory", args.project);
    }
    // Locate the index.yaml file and check its existance
    let mut file = PathBuf::new();
    file.push(fs::canonicalize(&args.project).unwrap().as_os_str());
    file.push("index.yaml");
    let yaml = match yaml::read_yaml(&file) {Ok(d) => d, Err(e) => {log::error!("{e:}");process::exit(1)},};
    // Basic validation the index.yaml file
    if let Err(e) = yaml::validate_index(&yaml) {log::error!("{e:}");process::exit(2)}
    // serde enforced validation
    let _yaml = match yaml::read_index(&file) {Ok(d) => d, Err(e) => {log::error!("{e:}");process::exit(1)},};
    log::info!("Project is valid");
    Ok(())
}
