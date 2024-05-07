use std::{fs, path::{PathBuf, Path}};
use clap::Args;
use anyhow::{Result, Error, bail, anyhow};
use k8s::{get_client, yaml, handlers::InstallHandler};
use package::{script, terraform};
use client::{AGENT, events};
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
    inst.update_status_start_plan(client.clone(), AGENT).await.map_err(|e| anyhow!("{e}"))?;
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

    //TODO: check for changes in that plan, set the status accordingly
    // run post-plan stage from rhai script if any
    script.run_post_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;
    // Upload the plan to the status->plan of the k8s Install Object
    inst.update_status_plan(client.clone(), AGENT, plan).await.map_err(|e| anyhow!("{e}"))?;
    events::report(AGENT, client,events::from(
        format!("Planning"),
        format!("Terraform plan for '{}.{}'",inst.namespace(),inst.name()),
        Some(format!("Terraform plan for '{}.{}' successfully completed",inst.namespace(),inst.name()))
    ), inst.object_ref(&())).await.unwrap();
    Ok(())
}

pub async fn run(args:&Parameters) -> Result<()> {
    let client = get_client().await;
    let mut installs = InstallHandler::new(&client.clone(), args.namespace.as_str());
    let inst = match installs.get(args.name.as_str()).await {Ok(d) => d, Err(e) => {
        events::report(AGENT, client, events::from_error(&anyhow!("{e}")), events::get_empty_ref()).await.unwrap();
        bail!("{e}");
    }};

    // Validate that the src parameter is a directory
    if ! Path::new(&args.src).is_dir() {
        inst.update_status_errors(client.clone(), AGENT, vec!(format!("{:?} is not a directory", args.src))).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&anyhow!("{:?} is not a directory", args.src)), inst.object_ref(&())).await.unwrap();
        bail!("{:?} is not a directory", args.src);
    }
    // Locate the index.yaml file and Load it
    let src = fs::canonicalize(&args.src).unwrap();
    let mut file = PathBuf::new();
    file.push(src.clone());
    file.push("index.yaml");
    let mut yaml = match yaml::read_index(&file) {Ok(d) => d, Err(e) => {
        inst.update_status_errors(client.clone(), AGENT, vec!(format!("{e}"))).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&e), inst.object_ref(&())).await.unwrap();
        return Err(e)
    }};
    // Start the script engine
    let mut script = script::Script::from_dir(&src.clone(), &"plan".to_string(), script::new_context(
        yaml.category.clone(),
        yaml.metadata.name.clone(),
        inst.name(),
        src.clone().into_os_string().into_string().unwrap(),
        src.clone().into_os_string().into_string().unwrap(),
        &yaml.get_values(&inst.options())
    ));
    match plan (&src, &mut script, client.clone(), &inst).await {Ok(_) => {Ok(())}, Err(e) => {
        inst.update_status_errors(client.clone(), AGENT, vec!(format!("{e}"))).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&e), inst.object_ref(&())).await.unwrap();
        Err(e)
    }}
}