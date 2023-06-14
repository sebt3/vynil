use std::{fs, path::{PathBuf, Path}};
use clap::Args;
use anyhow::{Result, Error, bail, anyhow};
use package::{yaml, script, terraform};
use client::{get_client, AGENT, events};
use kube::api::Resource;


#[derive(Args, Debug)]
pub struct Parameters {
    /// Terraform layer directory to destroy
    #[arg(short, long, value_name = "TF_DIR", env = "TF_DIR", default_value = "/src")]
    src: PathBuf,
    /// Install Namespace
    #[arg(short, long, env = "NAMESPACE", value_name = "NAMESPACE", default_value = "default")]
    namespace: String,
    /// Install Name
    #[arg(short='i', long, env = "NAME", value_name = "NAME")]
    name: String,
}

pub async fn destroy(src: &PathBuf, script: &mut script::Script, client: kube::Client, inst: &client::Install) -> Result<()> {
    inst.update_status_start_destroy(client.clone(), AGENT).await.map_err(|e| anyhow!("{e}"))?;
    // run pre-plan stage from rhai script if any
    let stage = "destroy".to_string();
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
    // run the terraform destroy command
    terraform::run_destroy(src).or_else(|e: Error| {bail!("{e}")})?;
    // run post-plan stage from rhai script if any
    script.run_post_stage(&stage).or_else(|e: Error| {bail!("{e}")}).or_else(|e: Error| {bail!("{e}")})?;
    inst.update_status_end_destroy(client.clone(), AGENT).await.map_err(|e| anyhow!("{e}"))?;
    events::report(AGENT, client,events::from(
        format!("Deleting {}",inst.name()),
        format!("Terraform destroy for `{}`",inst.name()),
        Some(format!("Terraform destroy for `{}` successfully completed",inst.name()
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
        let mut errors: Vec<String> = Vec::new();
        errors.push(format!("{e}"));
        inst.update_status_errors(client.clone(), AGENT, errors).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&e), inst.object_ref(&())).await.unwrap();
        return Err(e)
    }};
    // Start the script engine
    let mut file = PathBuf::new();
    file.push(src.clone());
    file.push("index.rhai");
    let mut script = script::Script::new(&file, script::new_context(
        yaml.category.clone(),
        yaml.metadata.name.clone(),
        inst.name(),
        src.clone().into_os_string().into_string().unwrap(),
        src.clone().into_os_string().into_string().unwrap(),
        &yaml.get_values(&inst.options())
    ));
    match destroy(&src, &mut script, client.clone(), &inst).await {Ok(_) => {Ok(())}, Err(e) => {
        inst.update_status_errors(client.clone(), AGENT, vec!(format!("{e}"))).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&e), inst.object_ref(&())).await.unwrap();
        Err(e)
    }}
}