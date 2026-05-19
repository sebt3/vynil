use clap::Args;
use common::{
    Error, Result,
    jukebox_file::{FileJukeBox, FileScanSpec},
    rhaihandler::Script,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Args, Debug, Serialize, Deserialize)]
pub struct Parameters {
    /// Fichier YAML de spec JukeBox (source + pull_secret)
    #[arg(short = 'S', long = "spec")]
    spec: PathBuf,
    /// Répertoire de sortie du cache
    #[arg(short = 'c', long = "cache-dir")]
    cache_dir: PathBuf,
    /// Répertoire des scripts agent
    #[arg(
        short = 's',
        long = "script-dir",
        env = "SCRIPT_DIRECTORY",
        default_value = "./agent/scripts"
    )]
    script_dir: PathBuf,
    /// Filtre partiel : "<category>" ou "<category>/<name>"
    #[arg(short = 'f', long = "filter", env = "SCAN_PACKAGE")]
    filter: Option<String>,
}

pub async fn run(args: &Parameters) -> Result<()> {
    let mut rhai = Script::new_file_scan(vec![
        format!("{}/boxes", args.script_dir.display()),
        format!("{}/lib", args.script_dir.display()),
    ]);

    let spec_content = tokio::fs::read_to_string(&args.spec)
        .await
        .map_err(Error::Stdio)?;
    let spec: FileScanSpec =
        serde_yaml::from_str(&spec_content).map_err(|e| Error::YamlError(e.to_string()))?;

    tokio::fs::create_dir_all(&args.cache_dir)
        .await
        .map_err(Error::Stdio)?;

    let file_box = FileJukeBox::new(spec, args.cache_dir.clone());
    rhai.ctx.set_value("box", file_box);
    rhai.set_dynamic(
        "args",
        &serde_json::json!({
            "file_scan": true,
            "cache_dir": args.cache_dir.to_string_lossy(),
            "script_dir": args.script_dir.to_string_lossy(),
            "filter": args.filter,
            "namespace": "",
        }),
    );

    let _ = rhai.run_file(&PathBuf::from(format!(
        "{}/boxes/scan.rhai",
        args.script_dir.display()
    )))?;
    Ok(())
}
