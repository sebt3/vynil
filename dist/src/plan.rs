use std::{fs, path::{PathBuf, Path}};
use clap::Args;
use anyhow::{Result, Error, bail};
use package::{yaml, script, terraform};

#[derive(Args, Debug)]
pub struct Parameters {
    /// Terraform layer directory to plan
    #[arg(short, long, value_name = "TF_DIR", env = "TF_DIR", default_value = "/src")]
    src: PathBuf,
    /// Install Namespace
    #[arg(short, long, env = "NAMESPACE", value_name = "NAMESPACE", default_value = "default")]
    namespace: String,
    /// Install Name
    #[arg(short='i', long, env = "NAME", value_name = "NAME")]
    name: String,
}

pub fn plan (src: &PathBuf, script: &mut script::Script) -> Result<()> {
    // run pre-plan stage from rhai script if any
    let stage = "plan".to_string();
    script.run_pre_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;
    // run the terraform plan command
    terraform::run_plan(src)?;
    // Get the json data from the plan file from terraform
    let _plan = terraform::get_plan(src).or_else(|e: Error| {bail!("{e}")}).unwrap();
    // run post-plan stage from rhai script if any
    script.run_post_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;
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
    let mut script = script::Script::from_dir(&src.clone(), &"plan".to_string(), script::new_context(
        yaml.category.clone(),
        yaml.metadata.name.clone(),
        yaml.metadata.name.clone(),
        src.clone().into_os_string().into_string().unwrap(),
        src.clone().into_os_string().into_string().unwrap(),
        &yaml.get_values(&serde_json::from_str("{}")?)
    ));
    match plan (&src, &mut script) {Ok(_) => {Ok(())}, Err(e) => {
        Err(e)
    }}
}