use clap::Args;
use common::{rhaihandler::Script, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Args, Debug, Serialize, Deserialize)]
pub struct Parameters {
    /// Source directory
    #[arg(
        short = 'o',
        long = "source",
        env = "SOURCE",
        value_name = "SOURCE",
        default_value = "/src"
    )]
    source: PathBuf,
    /// Agent script directory
    #[arg(
        short = 's',
        long = "script-dir",
        env = "SCRIPT_DIRECTORY",
        value_name = "SCRIPT_DIRECTORY",
        default_value = "./agent/scripts"
    )]
    script_dir: String,
}

pub async fn run(args: &Parameters) -> Result<()> {
    let mut rhai = Script::new(vec![
        format!("{}/scripts", args.source.to_string_lossy()),
        format!("{}/packages", args.script_dir),
        format!("{}/lib", args.script_dir),
    ]);
    rhai.set_dynamic("args", &serde_json::to_value(args).unwrap());
    let _ = rhai.eval(
        "import(\"update\") as update;\n\
        update::run(args);",
    )?;
    Ok(())
}
