use std::{fs, path::{PathBuf, Path}};
use clap::Args;
use anyhow::{Result, Error, bail};
use package::{yaml, script};

#[derive(Args, Debug)]
pub struct Parameters {
    /// Package sources
    #[arg(short, long, value_name = "SOURCE_DIR", default_value = "/work")]
    src: PathBuf,
    /// Install Namespace
    #[arg(short, long, env = "NAMESPACE", value_name = "NAMESPACE", default_value = "default")]
    namespace: String,
    /// Install Name
    #[arg(short='i', long, env = "NAME", value_name = "NAME")]
    name: String,
}

pub fn check (script: &mut script::Script) -> Result<()> {
    // run pre-check stage from rhai script if any
    let stage = "check".to_string();
    script.run_pre_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;
    Ok(())
}

pub fn run(args:&Parameters) -> Result<()> {

    // Validate that the src parameter is a directory
    if ! Path::new(&args.src).is_dir() {
        let mut errors: Vec<String> = Vec::new();
        errors.push(format!("{:?} is not a directory", args.src));
        bail!("{:?} is not a directory", args.src);
    }
    // Locate the index.yaml file and Load it
    let src = fs::canonicalize(&args.src).unwrap();
    let mut file = PathBuf::new();
    file.push(src.clone());
    file.push("index.yaml");
    let mut yaml = match yaml::read_index(&file) {Ok(d) => d, Err(e) => {
        return Err(e)
    }};
    // Start the script engine
    let mut script = script::Script::from_dir(&src.clone(), &"check".to_string(), script::new_base_context(
        yaml.category.clone(),
        yaml.metadata.name.clone(),
        yaml.metadata.name.clone(),
        &yaml.get_values(&serde_json::from_str("{}")?)
    ));
    match check (&mut script) {Ok(_) => {Ok(())}, Err(e) => {
        Err(e)
    }}
}