use std::{fs, path::{PathBuf, Path}};
use clap::Args;
use anyhow::{Result, Error, bail, anyhow};
use package::{yaml, script, terraform};
use client::{get_client, AGENT, events};
use kube::api::Resource;
use regex::Regex;
use std::{thread, time::Duration};
use serde_json::{Value, Map};


#[derive(Args, Debug)]
pub struct Parameters {
    /// Terraform layer directory to apply
    #[arg(short, long, value_name = "TF_DIR", env = "TF_DIR", default_value = "/src")]
    src: PathBuf,
    /// Install Namespace
    #[arg(short, long, env = "NAMESPACE", value_name = "NAMESPACE", default_value = "default")]
    namespace: String,
    /// Install Name
    #[arg(short='i', long, env = "NAME", value_name = "NAME")]
    name: String,
}

pub async fn install(src: &PathBuf, script: &mut script::Script, client: kube::Client, inst: &client::Install) -> Result<()> {
    if let Some(status) = inst.clone().status {
        let re = Regex::new(r"ing$").unwrap();
        if re.is_match(&status.status) {
            log::warn!("*concurrency problem*");
            log::warn!("Install {} might be already running since its status is {}", inst.metadata.name.clone().unwrap(), &status.status);
            log::warn!("Continue anyway");
        }
    }
    inst.update_status_start_install(client.clone(), AGENT).await.map_err(|e| anyhow!("{e}"))?;
    // run pre-install stage from rhai script if any
    let stage = "install".to_string();
    script.run_pre_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;

    // Check existing plan with advertised plan
    let plan = inst.plan();
    let mut path = PathBuf::new();
    path.push(src.clone());
    path.push("tf.plan");
    let have_plan = if ! plan.is_empty() && Path::new(&path).is_file() {
        // Verify that the existing plan is the advertised one
        let current = terraform::get_plan(src).or_else(|e: Error| {bail!("{e}")}).unwrap();
        if current != plan {
            log::warn!("Current plan file {:?} differ from the advertised one in Install {}", &path, inst.metadata.name.clone().unwrap());
        }
        true
    } else if plan.is_empty() {
        log::warn!("Plan file {:?} exist but is not documented on the Install {}. Sound doubious, continuing anyway", &path, inst.metadata.name.clone().unwrap());
        false
    } else {
        log::warn!("No plan file {:?} and none in Install {} either. Forcing initial install, hope for the best", &path, inst.metadata.name.clone().unwrap());
        false
    };
    // Handle state presence
    let tfstate = inst.tfstate();
    let mut path = PathBuf::new();
    path.push(src.clone());
    path.push("terraform.tfstate");
    if !tfstate.is_empty() && ! Path::new(&path).is_file() {
        if have_plan {
            log::warn!("Creating {:?} because it wasn't found but should have been there ?", &path);
            events::report(AGENT, client.clone(), events::from_error(&anyhow!("Creating {:?} because it wasn't found but should have been there ?", &path)), inst.object_ref(&())).await.unwrap();
        }
        // Create the tfstate file from the k8s data
        match std::fs::write(&path, serde_json::to_string_pretty(&tfstate).unwrap()) {Ok(_) => {}, Err(e) =>
            bail!("Error {} while generating: {}", e, path.display())
        };
    }

    // run the terraform install command
    terraform::run_apply(src).or_else(|e: Error| {bail!("{e}")})?;
    // run post-install stage from rhai script if any
    script.run_post_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;
    // Upload the tfstate to the status->tf_state of the k8s Install Object
    let content = match fs::read_to_string(path.clone()) {Ok(d) => d, Err(e) => bail!("Error {} while reading: {}", e, path.display())};
    let new_state = match serde_json::from_str(&content) {Ok(d) => d, Err(e) => bail!("Error {} while reading: {}", e, path.display())};
    inst.update_status_apply(client.clone(), AGENT, new_state, std::env::var("COMMIT_ID").unwrap_or_else(|_| String::new())).await.map_err(|e| anyhow!("{e}"))?;
    events::report(AGENT, client,events::from(
        format!("Installing"),
        format!("Terraform apply for '{}.{}'",inst.namespace(),inst.name()),
        Some(format!("Terraform apply for '{}.{}' successfully completed",inst.namespace(),inst.name()))
    ), inst.object_ref(&())).await.unwrap();
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
        inst.update_status_errors(client.clone(), AGENT, vec!(format!("{e}"))).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&e), inst.object_ref(&())).await.unwrap();
        return Err(e)
    }};
    // Start the script engine
    let mut script = script::Script::from_dir(&src.clone(), &"install".to_string(), script::new_context(
        yaml.category.clone(),
        yaml.metadata.name.clone(),
        inst.name(),
        src.clone().into_os_string().into_string().unwrap(),
        src.clone().into_os_string().into_string().unwrap(),
        &yaml.get_values(&inst.options())
    ));
    match install(&src, &mut script, client.clone(), &inst).await {Ok(_) => {Ok(())}, Err(e) => {
        let mut path = PathBuf::new();
        path.push(src.clone());
        path.push("terraform.tfstate");
        log::error!("Installation failed: {}", e);
        if Path::new(&path).is_file() {
            let content = match fs::read_to_string(path.clone()) {Ok(d) => d, Err(e) => bail!("Error {} while reading: {}", e, path.display())};
            let new_state:Map<String, Value> = match serde_json::from_str(&content) {Ok(d) => d, Err(e) => bail!("Error {} while reading: {}", e, path.display())};
            match inst.update_status_errors_tfstate(client.clone(), AGENT, vec!(format!("{e}")), new_state.clone()).await  {Ok(_) => {}, Err(e) => {
                log::warn!("Error {} while updating current tfstate while already managing errors. Potential tfstate lost !", e);
                log::warn!("Retrying in a second");
                thread::sleep(Duration::from_millis(1000));
                match inst.update_status_errors_tfstate(client.clone(), AGENT, vec!(format!("{e}")), new_state.clone()).await.map_err(|e| anyhow!("{e}")) {Ok(_) => {}, Err(e) => {
                    log::warn!("Error {} while updating current tfstate again. Sound realy bad this time", e);
                    log::warn!("Retrying in 5 secondS");
                    thread::sleep(Duration::from_millis(5000));
                    match inst.update_status_errors_tfstate(client.clone(), AGENT, vec!(format!("{e}")), new_state.clone()).await.map_err(|e| anyhow!("{e}")) {Ok(_) => {}, Err(e) => {
                        log::error!("TFSTATE LOST! reason: {}", e);
                        log::info!("{}", serde_json::to_string(&new_state)?);
                    }};
                }};
            }};
        } else {
            inst.update_status_errors(client.clone(), AGENT, vec!(format!("{e}"))).await.map_err(|e| anyhow!("{e}"))?;
        }
        events::report(AGENT, client, events::from_error(&e), inst.object_ref(&())).await.unwrap();
        Err(e)
    }}
}