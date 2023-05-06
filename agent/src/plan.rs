use std::{fs, path::{PathBuf, Path}};
use clap::Args;
use anyhow::{Result, Error, bail, anyhow};
use package::{yaml, script, terraform};
use client::{get_client, AGENT, events};
use kube::api::Resource;

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

pub async fn plan (src: &PathBuf, script: &mut script::Script, client: kube::Client, inst: &client::Install) -> Result<()> {
    // run pre-plan stage from rhai script if any
    let stage = "plan".to_string();
    script.run_pre_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;

    // Handle state presence
    let tfstate = inst.tfstate();
    if !tfstate.is_empty() {
        // Create the tfstate file from the k8s data
        let mut dest = PathBuf::new();
        dest.push(src.clone());
        dest.push("terraform.tfstate");
        match std::fs::write(&dest, serde_json::to_string_pretty(&tfstate).unwrap()) {Ok(_) => {}, Err(e) => 
            bail!("Error {} while generating: {}", e, dest.display())
        };
    }

    // run the terraform plan command
    terraform::run_plan(src)?;
    // Get the json data from the plan file from terraform
    let plan = terraform::get_plan(src).or_else(|e: Error| {bail!("{e}")}).unwrap();
    // run post-plan stage from rhai script if any
    script.run_post_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;
    // Upload the plan to the status->plan of the k8s Install Object
    inst.update_status_plan(client.clone(), AGENT, plan).await;
    events::report(AGENT, client,events::from(
        format!("Preparing {}",inst.name()),
        format!("Terraform plan for `{}`",inst.name()),
        Some(format!("Terraform plan for `{}` successfully completed",inst.name()
    ))), inst.object_ref(&())).await.unwrap();
    Ok(())
}

pub async fn run(args:&Parameters) -> Result<()> {
    let client = get_client().await;
    let mut installs = client::InstallHandler::new(client.clone(), args.namespace.as_str());
    let inst = match installs.get(args.name.as_str()).await {Ok(d) => d, Err(e) => {
        events::report(AGENT, client, events::from_error(&anyhow!("{e}")), events::get_empty_ref()).await.unwrap();
        bail!("{e}");
    }};

    // Validate that the src parameter is a directory
    if ! Path::new(&args.src).is_dir() {
        let mut errors: Vec<String> = Vec::new();
        errors.push(format!("{:?} is not a directory", args.src));
        inst.update_status_errors(client.clone(), AGENT, errors).await;
        events::report(AGENT, client, events::from_error(&anyhow!("{:?} is not a directory", args.src)), inst.object_ref(&())).await.unwrap();
        bail!("{:?} is not a directory", args.src);
    }
    // Locate the index.yaml file and Load it
    let src = fs::canonicalize(&args.src).unwrap();
    let mut file = PathBuf::new();
    file.push(src.clone());
    file.push("index.yaml");
    let mut yaml = match yaml::read_index(&file) {Ok(d) => d, Err(e) => {
        let mut errors: Vec<String> = Vec::new();
        errors.push(format!("{e}"));
        inst.update_status_errors(client.clone(), AGENT, errors).await;
        events::report(AGENT, client, events::from_error(&e), inst.object_ref(&())).await.unwrap();
        return Err(e)
    }};
    // Start the script engine
    let mut file = PathBuf::new();
    file.push(src.clone());
    file.push("index.rhai");
    let mut script = script::Script::new(&file, script::new_context(
        yaml.metadata.name.clone(),
        yaml.category.clone(),
        src.clone().into_os_string().into_string().unwrap(),
        src.clone().into_os_string().into_string().unwrap(),
        &yaml.get_values(&inst.options())
    ));
    match plan (&src, &mut script, client.clone(), &inst).await {Ok(_) => {Ok(())}, Err(e) => {
        let mut errors: Vec<String> = Vec::new();
        errors.push(format!("{e}"));
        inst.update_status_errors(client.clone(), AGENT, errors).await;
        events::report(AGENT, client, events::from_error(&e), inst.object_ref(&())).await.unwrap();
        Err(e)
    }}
}