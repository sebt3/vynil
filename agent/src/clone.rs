use anyhow::{anyhow, bail, Error, Result};
use clap::Args;
use client::{events, get_client, AGENT};
use k8s::distrib::DistribComponent;
use kube::api::Resource;
use package::{shell, yaml};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Args, Debug)]
pub struct Parameters {
    /// Directory to clone into
    #[arg(
        short,
        long,
        env = "GIT_ROOT",
        value_name = "GIT_ROOT",
        default_value = "/work"
    )]
    dir: PathBuf,
    /// Distrib name
    #[arg(
        short,
        long,
        env = "DIST_NAME",
        value_name = "DIST_NAME",
        default_value = "core"
    )]
    name: String,
}

fn have_index(dir: &Path) -> bool {
    let mut index: PathBuf = PathBuf::new();
    index.push(dir);
    index.push("index.yaml");
    Path::new(&index).is_file()
}

fn get_commit_id(component_dir: &PathBuf) -> Result<String> {
    let dir_path = component_dir.as_os_str().to_string_lossy();

    let files = fs::read_dir(component_dir)?
        .filter(|r| r.is_ok()) // Get rid of Err variants for Result<DirEntry>
        .map(|r| r.unwrap().path()) // This is safe, since we only have the Ok variants
        .filter(|r| r.is_file());
    let mut hashes = Vec::new();
    // get the commit id of each files
    for file in files {
        let commit = match shell::get_output(&format!(
            "cd {:?};git log --format=\"%H\" -n 1 -- {:?}",
            dir_path, file
        )) {
            Ok(d) => d,
            Err(e) => {
                bail!("{e}")
            }
        };
        if !hashes.contains(&commit) {
            hashes.push(commit);
        }
    }
    if hashes.len() == 1 {
        return Ok(hashes[0].clone());
    } else if hashes.is_empty() {
        bail!("No commit found");
    }
    // find the most recent commit from that list
    let commit_list = match shell::get_output(&format!("cd {:?};git log --format=\"%H\"", dir_path)) {
        Ok(d) => d,
        Err(e) => {
            bail!("{e}")
        }
    };
    let mut found = String::new();
    let mut current_id = 0;
    for hash in hashes {
        for (i, id) in commit_list.lines().enumerate() {
            if id == hash {
                if found.is_empty() || current_id > i {
                    found = hash.to_string();
                    current_id = i;
                }
                break;
            }
        }
    }
    if found.is_empty() {
        bail!("No commit found");
    }
    Ok(found)
}

pub async fn clone(target: &PathBuf, client: kube::Client, dist: &client::Distrib) -> Result<()> {
    let url = dist.spec.url.clone();
    let mut dot_git: PathBuf = PathBuf::new();
    dot_git.push(target.clone());
    dot_git.push(".git");
    if dist.insecure() {
        shell::run_log(&"git config --global http.sslVerify false".into())
            .or_else(|e: Error| bail!("{e}"))?;
    }
    let action = if Path::new(&dot_git).is_dir() {
        // if a .git directory exist, run git pull
        let mut branch_manage: String = "".to_owned();
        if dist.branch() != "" {
            branch_manage.push_str(&format!(
                "git switch {branch}; git reset --hard origin/{branch}",
                branch = dist.branch()
            ))
        } else {
            branch_manage.push_str("RBRANCH=$(git symbolic-ref refs/remotes/origin/HEAD | sed 's@^refs/remotes/origin/@@'); git switch ${RBRANCH}; git reset --hard origin/${RBRANCH}")
        }
        shell::run_log(&format!(
            "set -e ; cd {target} ; git remote set-url origin {url} ; git fetch ; {command}",
            target = target.display(),
            url = url,
            command = branch_manage
        ))
        .or_else(|e: Error| bail!("{e}"))?;
        format!("git pull for {}", dist.name())
    } else {
        // Run git clone
        let mut branch_manage: String = "".to_owned();
        if dist.branch() != "" {
            branch_manage.push_str(&format!(
                "git clone {url} -b {branch} .",
                branch = dist.branch(),
                url = url
            ))
        } else {
            branch_manage.push_str(&format!(
                "git clone {url} .",
                url = url
            ))
        }
        shell::run_log(&format!(
            "set -e ; cd {target} ; {command}",
            target = target.display(),
            command = branch_manage
        ))
        .or_else(|e: Error| bail!("{e}"))?;
        format!("git clone for {}", dist.name())
    };
    let mut categories = HashMap::new();
    let c_dirs = fs::read_dir(target)?
        .filter(|r| r.is_ok()) // Get rid of Err variants for Result<DirEntry>
        .map(|r| r.unwrap().path()) // This is safe, since we only have the Ok variants
        .filter(|r| r.is_dir() && r.file_name().unwrap().to_str().unwrap() != ".git");
    for c_subdir in c_dirs {
        let category = c_subdir.file_name().unwrap().to_str().unwrap().to_string();
        log::info!("looking for components in: {:}", category);
        let mut comps = HashMap::new();

        let pkgs = fs::read_dir(c_subdir)?
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
            let yaml = yaml::read_index(&index).or_else(|e: Error| bail!("{e}"))?;
            let mut script_file: PathBuf = PathBuf::new();
            script_file.push(comp_dir.clone());
            script_file.push("check.rhai");
            let check = if Path::new(&script_file.clone()).is_file() {
                Some(fs::read_to_string(script_file).unwrap())
            } else {
                None
            };
            comps.insert(
                comp_name,
                DistribComponent::new(
                    get_commit_id(&comp_dir.clone()).or_else(|e: Error| bail!("{e}"))?,
                    yaml.metadata.description,
                    yaml.options.into_iter().collect(),
                    yaml.dependencies,
                    yaml.providers,
                    check
                ),
            );
        }
        categories.insert(category, comps);
    }
    dist.update_status_components(client.clone(), AGENT, categories)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    events::report(
        AGENT,
        client,
        events::from(format!("Preparing {}", dist.name()), action.clone(), Some(action)),
        dist.object_ref(&()),
    )
    .await
    .unwrap();
    Ok(())
}

pub async fn run(args: &Parameters) -> Result<()> {
    let client = get_client().await;
    let mut distribs = client::DistribHandler::new(client.clone());
    let dist = match distribs.get(args.name.as_str()).await {
        Ok(d) => d,
        Err(e) => {
            events::report(
                AGENT,
                client,
                events::from_error(&anyhow!("{e}")),
                events::get_empty_ref(),
            )
            .await
            .unwrap();
            bail!("{e}");
        }
    };
    // Validate that the dir parameter is a directory
    if !Path::new(&args.dir).is_dir() {
        let mut errors: Vec<String> = Vec::new();
        errors.push(format!("{:?} is not a directory", args.dir));
        dist.update_status_errors(client.clone(), AGENT, errors)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        events::report(
            AGENT,
            client,
            events::from_error(&anyhow!("{:?} is not a directory", args.dir)),
            dist.object_ref(&()),
        )
        .await
        .unwrap();
        bail!("{:?} is not a directory", args.dir);
    }
    let target = std::fs::canonicalize(&args.dir).unwrap();
    match clone(&target, client.clone(), &dist).await {
        Ok(_) => Ok(()),
        Err(e) => {
            let mut errors: Vec<String> = Vec::new();
            errors.push(format!("{e}"));
            dist.update_status_errors(client.clone(), AGENT, errors)
                .await
                .map_err(|e| anyhow!("{e}"))?;
            events::report(AGENT, client, events::from_error(&e), dist.object_ref(&()))
                .await
                .unwrap();
            Err(e)
        }
    }
}
