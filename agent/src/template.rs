use std::{fs, path::{PathBuf, Path}};
use clap::Args;
use regex::Regex;
use handlebars::Handlebars;
use anyhow::{Result, Error, bail, anyhow};
use package::{yaml, script, terraform};
use client::{get_client, AGENT, events};
use kube::api::Resource;

#[derive(Args, Debug)]
pub struct Parameters {
    /// Source directory
    #[arg(short, long, value_name = "SOURCE_DIR", env = "SOURCE_DIR", default_value = "/src")]
    source: PathBuf,
    /// Destination directory
    #[arg(short, long, value_name = "DEST_DIR", env = "DEST_DIR", default_value = "/dest")]
    dest: PathBuf,
    /// Install Namespace
    #[arg(short, long, env = "NAMESPACE", value_name = "NAMESPACE", default_value = "default")]
    namespace: String,
    /// Install Name
    #[arg(short='i', long, env = "NAME", value_name = "NAME")]
    name: String,
}

pub async fn template(src: PathBuf, dest: PathBuf, client: kube::Client,
    inst: &client::Install,
    yaml: &yaml::Component,
    config:&serde_json::Map<String, serde_json::Value>,
    script: &mut script::Script) -> Result<()> {
    let providers = yaml.providers.clone();
    inst.update_status_start_template(client.clone(), AGENT).await.map_err(|e| anyhow!("{e}"))?;
    let reg = Handlebars::new();
    // run pre-template stage from rhai script if any
    let stage = "template".to_string();
    script.run_pre_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;
    // look source directory
    let re_rhai = Regex::new(r"^index\.rhai$").unwrap();
    let re_hbs = Regex::new(r"\.hbs$").unwrap();
    let re_yml = Regex::new(r"\.yaml$").unwrap();
    let re_tf = Regex::new(r"\.tf$").unwrap();
    for file in fs::read_dir(src).unwrap() {
        let path = file.unwrap().path();
        let filename = path.file_name().unwrap().to_str().unwrap();
        if re_hbs.is_match(filename) {
            // Instanciate every templates based on ENV values and (source)/index.yaml to a (dest) directory
            let src_content = fs::read_to_string(path.clone())
                .expect("Should have been able to read the file");
            let mut name = String::from(filename);
            name.truncate(name.len() - 4);
            let mut dest_path = PathBuf::new();
            dest_path.push(dest.clone());
            dest_path.push(name);
            log::debug!("Generating {:?}",dest_path);
            fs::write(dest_path, reg.render_template(src_content.as_str(), config)?)?;
        } else if re_yml.is_match(filename) || re_tf.is_match(filename) || re_rhai.is_match(filename) {
            // copy the remaining yaml and tf file to the same (dest) directory
            let mut dest_path = PathBuf::new();
            dest_path.push(dest.clone());
            dest_path.push(path.file_name().unwrap());
            fs::copy(path, dest_path).unwrap();
        }
    }
    terraform::gen_providers(&dest, providers).or_else(|e: Error| {bail!("{e}")})?;
    terraform::gen_variables(&dest, yaml, config, inst.spec.category.as_str(),inst.spec.component.as_str(), inst.name().as_str()).or_else(|e: Error| {bail!("{e}")})?;
    terraform::gen_datas(&dest).or_else(|e: Error| {bail!("{e}")})?;
    terraform::gen_ressources(&dest).or_else(|e: Error| {bail!("{e}")})?;
    terraform::gen_tfvars(&dest, config).or_else(|e: Error| {bail!("{e}")})?;
    // run post-template stage from rhai script if any
    script.run_post_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;
    inst.update_status_end_template(client.clone(), AGENT).await.map_err(|e| anyhow!("{e}"))?;
    match events::report(AGENT, client,events::from(
        format!("Installing {}",inst.name()),
        format!("Generating templates for `{}`",inst.name()),
        Some(format!("Generating templates for `{}` successfully completed",inst.name()
    ))), inst.object_ref(&())).await  {Ok(_) => {}, Err(e) =>
        {log::warn!("While sending event we got {:?}",e)}
    };
    Ok(())
}

pub async fn run(args:&Parameters) -> Result<()> {
    let client = get_client().await;
    let mut installs = client::InstallHandler::new(client.clone(), args.namespace.as_str());
    let inst = match installs.get(args.name.as_str()).await{Ok(d) => d, Err(e) => {
        events::report(AGENT, client, events::from_error(&anyhow!("{e}")), events::get_empty_ref()).await.unwrap();
        bail!("{e}");
    }};
    // Validate that the dest parameter is a directory
    if ! Path::new(&args.dest).is_dir() {
        inst.update_status_errors(client.clone(), AGENT, vec!(format!("{:?} is not a directory", args.dest))).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&anyhow!("{:?} is not a directory", args.dest)), inst.object_ref(&())).await.unwrap();
        bail!("{:?} is not a directory", args.dest);
    }
    // Validate that the source parameter is a directory
    if ! Path::new(&args.source).is_dir() {
        inst.update_status_errors(client.clone(), AGENT, vec!(format!("{:?} is not a directory", args.source))).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&anyhow!("{:?} is not a directory", args.source)), inst.object_ref(&())).await.unwrap();
        bail!("{:?} is not a directory", args.source);
    }
    // Locate the index.yaml file and Load it
    let src = fs::canonicalize(&args.source).unwrap();
    let dest = fs::canonicalize(&args.dest).unwrap();
    let mut file = PathBuf::new();
    file.push(src.clone());
    file.push("index.yaml");
    let mut yaml: yaml::Component = match yaml::read_index(&file){Ok(d) => d, Err(e) => {
        events::report(AGENT, client, events::from_error(&anyhow!("{e}")), events::get_empty_ref()).await.unwrap();
        bail!("{e}");
    }};
    // Start the script engine
    let mut file = PathBuf::new();
    file.push(src.clone());
    file.push("index.rhai");
    let mut script = script::Script::new(&file, script::new_context(
        yaml.category.clone(),
        yaml.metadata.name.clone(),
        args.name.clone(),
        src.clone().into_os_string().into_string().unwrap(),
        dest.clone().into_os_string().into_string().unwrap(),
        &yaml.get_values(&inst.options())
    ));
    match template(src, dest, client.clone(), &inst, &yaml.clone(), &yaml.get_values(&inst.options()), &mut script).await {Ok(_) => {Ok(())}, Err(e) => {
        inst.update_status_errors(client.clone(), AGENT, vec!(format!("{e}"))).await.map_err(|e| anyhow!("{e}"))?;
        events::report(AGENT, client, events::from_error(&e), inst.object_ref(&())).await.unwrap();
        Err(e)
    }}
}