use std::{fs, path::{PathBuf, Path}};
use clap::Args;
use regex::Regex;
use handlebars::Handlebars;
use anyhow::{Result, Error, bail};
use package::{yaml::{self, Component}, script, terraform};

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

pub fn template(src: PathBuf, dest: PathBuf, yaml: Component, config:&serde_json::Map<String, serde_json::Value>, script: &mut script::Script, providers: Option<yaml::Providers>) -> Result<()> {
    let reg = Handlebars::new();
    // run pre-template stage from rhai script if any
    let stage = "template".to_string();
    script.run_pre_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;
    // look source directory
    let re_rhai = Regex::new(r"\.rhai$").unwrap();
    let re_hbs = Regex::new(r"\.hbs$").unwrap();
    let re_yml = Regex::new(r"\.yaml$").unwrap();
    let re_tpl = Regex::new(r"\.tpl$").unwrap();
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
        } else if re_yml.is_match(filename) || re_tf.is_match(filename) || re_rhai.is_match(filename)  || re_tpl.is_match(filename) {
            // copy the remaining yaml and tf file to the same (dest) directory
            let mut dest_path = PathBuf::new();
            dest_path.push(dest.clone());
            dest_path.push(path.file_name().unwrap());
            fs::copy(path, dest_path).unwrap();
        }
    }
    terraform::gen_providers(&dest, providers).or_else(|e: Error| {bail!("{e}")})?;
    terraform::gen_variables(&dest, &yaml, config,yaml.category.as_str(), yaml.metadata.name.as_str(), yaml.metadata.name.as_str()).or_else(|e: Error| {bail!("{e}")})?;
    if terraform::have_datas(&dest) {
        terraform::gen_ressources(&dest).or_else(|e: Error| {bail!("{e}")})?;
    }
    terraform::gen_tfvars(&dest, config, None).or_else(|e: Error| {bail!("{e}")})?;
    // run post-template stage from rhai script if any
    script.run_post_stage(&stage).or_else(|e: Error| {bail!("{e}")})?;
    Ok(())
}

pub fn run(args:&Parameters) -> Result<()> {
    // Validate that the dest parameter is a directory
    if ! Path::new(&args.dest).is_dir() {
        bail!("{:?} is not a directory", args.dest);
    }
    // Validate that the source parameter is a directory
    if ! Path::new(&args.source).is_dir() {
        bail!("{:?} is not a directory", args.source);
    }
    // Locate the index.yaml file and Load it
    let src = fs::canonicalize(&args.source).unwrap();
    let dest = fs::canonicalize(&args.dest).unwrap();
    let mut file = PathBuf::new();
    file.push(src.clone());
    file.push("index.yaml");
    let mut yaml = match yaml::read_index(&file){Ok(d) => d, Err(e) => {
        bail!("{e}");
    }};
    // Start the script engine
    let mut script = script::Script::from_dir(&src.clone(), &"template".to_string(), script::new_context(
        yaml.category.clone(),
        yaml.metadata.name.clone(),
        yaml.metadata.name.clone(),
        src.clone().into_os_string().into_string().unwrap(),
        dest.clone().into_os_string().into_string().unwrap(),
        &yaml.get_values(&serde_json::from_str("{}")?)
    ));
    match template(src, dest, yaml.clone(), &yaml.get_values(&serde_json::from_str("{}")?), &mut script, yaml.providers.clone()) {Ok(_) => {Ok(())}, Err(e) => {
        Err(e)
    }}
}