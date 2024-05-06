use std::{fs, path::{PathBuf, Path}};
use anyhow::{Result, bail};
use clap::Args;
use regex::Regex;
use package::{yaml, script, template};

#[derive(Args, Debug)]
pub struct Parameters {
    /// Project source directory
    #[arg(short, long, value_name = "SOURCE_DIR", default_value = "/work")]
    project: PathBuf,
    /// Distribution destination directory
    #[arg(short, long, value_name = "DIST_DIR", default_value = "/dist")]
    dist: PathBuf,
}

fn explode(src: &PathBuf, dest: &Path, config: &serde_json::Map<String, serde_json::Value>, is_template: bool) -> Result<()> {
    let content = fs::read_to_string(src)
        .expect("Should have been able to read the file");
    let parts = content.split("
---
");
    for i in parts {
        let str = i.to_string();
        let str = str.trim();
        if str.split('\n').count() > 4 {
            let yaml: serde_yaml::Value = match serde_yaml::from_str(str) {Ok(d) => d, Err(e) => {log::error!("'{e:}' while parsing yaml chuck from {}: '{str}'", src.display());std::process::exit(1)},};
            let kind = yaml["kind"].as_str().map(std::string::ToString::to_string).unwrap();
            let version = yaml["apiVersion"].as_str().map(std::string::ToString::to_string).unwrap();
            let version = version.replace('/', "_");
            let name = yaml["metadata"]["name"].as_str().map(std::string::ToString::to_string).unwrap();
            let name = template::template(name.as_str(), config).unwrap();
            let filename =  if str.to_string().contains("{{") && is_template {
                format!("{}_{}_{}.yaml.hbs", version, kind, name)
            } else {
                format!("{}_{}_{}.yaml", version, kind, name)
            };
            let mut file  = PathBuf::new();
            file.push(dest);
            file.push(&filename);
            match std::fs::write(file.clone(), str) {Ok(_) => {}, Err(e) => bail!("Error {} while generating: {}", e, file.display()),};
        }
    }
    Ok(())
}

pub fn run(args:&Parameters) -> Result<()> {
    // Validate that the project parameter is a directory
    if ! Path::new(&args.project).is_dir() {
        bail!("{:?} is not a directory", args.project);
    }
    if ! Path::new(&args.dist).is_dir() {
        bail!("{:?} is not a directory", args.dist);
    }
    // Locate the index.yaml file and Load it
    let path = fs::canonicalize(&args.project).unwrap();
    let mut file = PathBuf::new();
    file.push(path.clone());
    file.push("index.yaml");
    // let yaml = match yaml::read_yaml(&file) {
    //     Ok(d) => d, Err(e) => {bail!("{e:}")},
    // };
    // // Validate the index.yaml file
    // yaml::validate_index(&yaml)?;
    // Final validation
    log::debug!("Validating index.yaml");
    let mut yaml = match yaml::read_index(&file) {Ok(d) => d, Err(e) => {log::error!("{e:} while validating generated index.yaml");std::process::exit(1)},};
    yaml.update_options_from_defaults()?;
    log::debug!("Validating index.yaml : ok");
    // Create the dest directory if not existing
    let dist = fs::canonicalize(&args.dist).unwrap();
    let mut tmp = PathBuf::new();
    tmp.push(dist);
    tmp.push(yaml.category.clone());
    tmp.push(yaml.metadata.name.clone());
    match fs::remove_dir_all(&tmp) {Ok(_) => {}, Err(_e) => {log::debug!("{:?} did not exist", tmp)}}
    fs::create_dir_all(&tmp)?;
    let dest_dir = fs::canonicalize(&tmp).unwrap();

    // Start the script engine
    let mut script = script::Script::from_dir(&path.clone(), &"pack".to_string(), script::new_context(
        yaml.category.clone(),
        yaml.metadata.name.clone(),
        yaml.metadata.name.clone(),
        path.clone().into_os_string().into_string().unwrap(),
        dest_dir.clone().into_os_string().into_string().unwrap(),
        &yaml.get_values(&serde_json::Map::new())
    ));
    // run the pre-pack stage if any
    let stage = "pack".to_string();
    match script.run_pre_stage(&stage) {Ok(_) => {}, Err(e) => {return Err(e)}}
    // look source directory
    let mut copies: Vec<PathBuf> = Vec::new();
    let re_kusto = Regex::new(r"^kustomization\.yaml$").unwrap();
    let re_kustohbs = Regex::new(r"^kustomization\.yaml\.hbs$").unwrap();
    let re_rhai = Regex::new(r"\.rhai$").unwrap();
    let re_hbs = Regex::new(r"\.hbs$").unwrap();
    let re_ymlhbs = Regex::new(r"\.yaml\.hbs$").unwrap();
    let re_yml = Regex::new(r"\.yaml$").unwrap();
    let re_tf = Regex::new(r"\.tf$").unwrap();
    let re_def = Regex::new(r"^index\.yaml$").unwrap();
    let mut use_kusto = false;
    let mut use_templates = false;
    for file in fs::read_dir(path.clone()).unwrap() {
        let path = file.unwrap().path();
        let filename = path.file_name().unwrap().to_str().unwrap();
        if re_def.is_match(filename) {
            continue;
        } else if re_kusto.is_match(filename) || re_kustohbs.is_match(filename) {
            use_kusto = true;
            if re_kustohbs.is_match(filename) {
                use_templates = true;
            }
            copies.push(path);
        } else if re_yml.is_match(filename) {
            log::debug!("exploding yaml file: {}", filename);
            match explode(&path, &dest_dir, &yaml.get_values(&serde_json::Map::new()), false)  {Ok(_) => {}, Err(e) => {return Err(e)}}
        } else if re_ymlhbs.is_match(filename) {
            use_templates = true;
            log::debug!("exploding handlebars template file: {}", filename);
            match explode(&path, &dest_dir, &yaml.get_values(&serde_json::Map::new()), true)  {Ok(_) => {}, Err(e) => {return Err(e)}}
        } else if re_hbs.is_match(filename) {
            use_templates = true;
            copies.push(path);
        } else if re_tf.is_match(filename) ||
                (re_rhai.is_match(filename) && (script.have_stage("install") || script.have_stage("destroy") || script.have_stage("plan") || script.have_stage("template"))) {
            copies.push(path);
        }
    }
    if use_kusto {
        log::warn!("Having a kustomization.yaml file will cause the terraform plan/install stages to fail!");
        log::warn!("Make sure the file doesn't survive your install process before terraform plan is fired (hint, use a use pre_plan rhai stage)");
    }
    if use_templates {
        log::warn!("Using mustaches templates is not recommanded, you should migrate to terraform base templating instead.");
    }
    log::debug!("Preparing final index.yaml");
    let mut dest_path: PathBuf = PathBuf::new();
    dest_path.push(dest_dir.clone());
    dest_path.push("index.yaml");
    yaml.write_self_to(dest_path.clone())?;
    log::debug!("Updated final index.yaml");
    let mut yaml = match yaml::read_index(&dest_path) {Ok(d) => d, Err(e) => {log::error!("{e:}");std::process::exit(1)},};
    let mut script = script::Script::from_dir(&path.clone(), &"pack".to_string(), script::new_context(
        yaml.category.clone(),
        yaml.metadata.name.clone(),
        yaml.metadata.name.clone(),
        path.into_os_string().into_string().unwrap(),
        dest_dir.clone().into_os_string().into_string().unwrap(),
        &yaml.get_values(&serde_json::Map::new())
    ));
    match yaml.validate() {Ok(_) => {}, Err(e) => {return Err(e)}}

    // Copy all valids and selected files to dist
    for path in copies {
        let mut dest_path: PathBuf = PathBuf::new();
        dest_path.push(dest_dir.clone());
        dest_path.push(path.file_name().unwrap());
        fs::copy(path, dest_path).unwrap();
    }
    // run the post-pack stage if any
    match script.run_post_stage(&stage) {Ok(_) => {}, Err(e) => {return Err(e)}}
    Ok(())
}
