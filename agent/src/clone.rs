use clap::Args;
use anyhow::{Result, Error, bail, anyhow};
use std::{fs, path::{PathBuf, Path}};
use package::{shell, yaml};
use client::{get_client, AGENT, events};
use kube::api::Resource;
use std::collections::HashMap;

#[derive(Args, Debug)]
pub struct Parameters {
    /// Directory to clone into
    #[arg(short, long, env = "GIT_ROOT", value_name = "GIT_ROOT", default_value = "/work")]
    dir: PathBuf,
    /// Distrib name
    #[arg(short, long, env = "DIST_NAME", value_name = "DIST_NAME", default_value = "base")]
    name: String,
}

fn have_index(dir: &Path) -> bool {
    let mut index: PathBuf = PathBuf::new();
    index.push(dir);
    index.push("index.yaml");
    Path::new(&index).is_file()
}

pub async fn clone (target: &PathBuf, client: kube::Client, dist: &client::Distrib) -> Result<()> {
    let url = dist.spec.url.clone();
    let mut dot_git: PathBuf = PathBuf::new();
    dot_git.push(target.clone());
    dot_git.push(".git");
    if dist.insecure() {
        shell::run_log(&"git config --global http.sslVerify false".into()).or_else(|e: Error| {bail!("{e}")})?;
    }
    // TODO: Support selecting branch
    // TODO: Support git login somehow
    let action = if Path::new(&dot_git).is_dir() {
        // if a .git directory exist, run git pull
        shell::run_log(&format!("cd {:?};git pull", target)).or_else(|e: Error| {bail!("{e}")})?;
        // TODO: Detect changes, if some, mass-plan the changes
        format!("git pull for {}",dist.name())
    } else {
        // Run git clone
        shell::run_log(&format!("cd {:?};find;git clone {:?} .", target, url)).or_else(|e: Error| {bail!("{e}")})?;
        format!("git clone for {}",dist.name())
    };
    let mut categories = HashMap::new();
    let c_dirs = fs::read_dir(target)?
        .into_iter()
        .filter(|r| r.is_ok()) // Get rid of Err variants for Result<DirEntry>
        .map(|r| r.unwrap().path()) // This is safe, since we only have the Ok variants
        .filter(|r| r.is_dir() && r.file_name().unwrap().to_str().unwrap() != ".git");
    for c_subdir in c_dirs {
        let category = c_subdir.file_name().unwrap().to_str().unwrap().to_string();
        log::info!("looking for components in: {:}", category);
        let mut comps = HashMap::new();

        let pkgs = fs::read_dir(c_subdir)?
            .into_iter()
            .filter(|r| r.is_ok()) // Get rid of Err variants for Result<DirEntry>
            .map(|r| r.unwrap().path()) // This is safe, since we only have the Ok variants
            .filter(|r| r.is_dir())
            .filter(|r| have_index(r));
        for comp_dir in pkgs {
            let comp_name = comp_dir.file_name().unwrap().to_str().unwrap().to_string();
            log::info!("found component {:} in: {:}", comp_name, category);
            let mut index: PathBuf = PathBuf::new();
            index.push(comp_dir.clone());
            index.push("index.yaml");
            comps.insert(comp_name, yaml::read_index(&index).or_else(|e: Error| {bail!("{e}")})?);
        }
        categories.insert(category, comps);
    }
    dist.update_status_components(client.clone(), AGENT, categories).await.map_err(|e| anyhow!("{e}"))?;
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
        dist.update_status_errors(client.clone(), AGENT, errors).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&anyhow!("{:?} is not a directory", args.dir)), dist.object_ref(&())).await.unwrap();
        bail!("{:?} is not a directory", args.dir);
    }
    let target = std::fs::canonicalize(&args.dir).unwrap();
    match clone (&target, client.clone(), &dist).await {Ok(_) => {Ok(())}, Err(e) => {
        let mut errors: Vec<String> = Vec::new();
        errors.push(format!("{e}"));
        dist.update_status_errors(client.clone(), AGENT, errors).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&e), dist.object_ref(&())).await.unwrap();
        Err(e)
    }}
}