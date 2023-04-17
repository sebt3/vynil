use clap::Args;
use anyhow::{Result, Error, bail, anyhow};
use std::{path::{PathBuf, Path}};
use package::shell;
use client::{get_client, AGENT, events};
use kube::api::Resource;

#[derive(Args, Debug)]
pub struct Parameters {
    /// Directory to clone into
    #[arg(short, long, env = "GIT_ROOT", value_name = "GIT_ROOT", default_value = "/work")]
    dir: PathBuf,
    /// Distrib name
    #[arg(short, long, env = "DIST_NAME", value_name = "DIST_NAME", default_value = "base")]
    name: String,
}
pub async fn clone (target: &PathBuf, client: kube::Client, dist: &client::Distrib) -> Result<()> {
    let url = dist.spec.url.clone();
    let mut dot_git = PathBuf::new();
    dot_git.push(target.clone());
    dot_git.push(".git");
    if dist.insecure() {
        shell::run_log(&"git config --global http.sslVerify false".into()).or_else(|e: Error| {bail!("{e}")})?;
    }
    // TODO: Support selecting branch
    let action = if Path::new(&dot_git).is_dir() {
        // if a .git directory exist, run git pull
        shell::run_log(&format!("cd {:?};git pull", target)).or_else(|e: Error| {bail!("{e}")})?;
        // TODO: Detect changes, if some, mass-plan the changes
        format!("git pull for {}",dist.name())
    } else {
        // Run git clone
        shell::run_log(&format!("cd {:?};git clone {:?} .", target, url)).or_else(|e: Error| {bail!("{e}")})?;
        format!("git clone for {}",dist.name())
    };
    // TODO: Collect found packages
    dist.update_status_components(client.clone(), AGENT,Vec::new()).await;
    events::report(AGENT, client,events::from(
        format!("Preparing {}", dist.name()),action.clone(),
        Some(action)
    ), dist.object_ref(&())).await.unwrap();
    Ok(())
}

pub async fn run(args:&Parameters) -> Result<()> {
    let client = get_client().await;
    let mut distribs = client::DistribHandler::new(client.clone());
    let dist = match distribs.get(args.name.as_str()).await {Ok(d) => d, Err(e) => {
        events::report(AGENT, client, events::from_error(&anyhow!("{e}")), events::get_empty_ref()).await.unwrap();
        bail!("{e}");
    }};
     // Validate that the dir parameter is a directory
     if ! Path::new(&args.dir).is_dir() {
        let mut errors: Vec<String> = Vec::new();
        errors.push(format!("{:?} is not a directory", args.dir));
        dist.update_status_errors(client.clone(), AGENT, errors).await;
        events::report(AGENT, client, events::from_error(&anyhow!("{:?} is not a directory", args.dir)), dist.object_ref(&())).await.unwrap();
        bail!("{:?} is not a directory", args.dir);
    }
    let target = std::fs::canonicalize(&args.dir).unwrap();
    match clone (&target, client.clone(), &dist).await {Ok(_) => {Ok(())}, Err(e) => {
        let mut errors: Vec<String> = Vec::new();
        errors.push(format!("{e}"));
        dist.update_status_errors(client.clone(), AGENT, errors).await;
        events::report(AGENT, client, events::from_error(&e), dist.object_ref(&())).await.unwrap();
        Err(e)
    }}
}