use clap::Args;
use common::{rhaihandler::Script, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(clap::ValueEnum, Clone, Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildType {
    /// Major new version
    Major,
    /// Minor new version
    Minor,
    /// Patch new version
    Patch,
    /// Beta new version
    Beta,
    #[default]
    /// Alpha new version
    Alpha,
}

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
    /// Temporary directory
    #[arg(
        short = 't',
        long = "temporary",
        env = "TEMPORARY",
        value_name = "TEMPORARY",
        default_value = "/tmp/package"
    )]
    temp: PathBuf,
    /// Build type
    #[arg(
        short = 'b',
        long = "build-type",
        env = "BUILD_TYPE",
        value_name = "BUILD_TYPE",
        default_value = "alpha"
    )]
    build: BuildType,
    /// version
    #[arg(long = "tag", env = "TAG", value_name = "TAG", default_value = "")]
    tag: String,
    /// Registry
    #[arg(
        short = 'r',
        long = "registry",
        env = "REGISTRY",
        value_name = "REGISTRY",
        default_value = "oci.solidite.fr"
    )]
    registry: String,
    /// Build type
    #[arg(short = 'n', long = "name", env = "IMAGE_NAME", value_name = "IMAGE_NAME")]
    repository: String,
    /// Username
    #[arg(
        short = 'u',
        long = "username",
        env = "USERNAME",
        value_name = "USERNAME",
        default_value = ""
    )]
    username: String,
    /// Password
    #[arg(
        short = 'p',
        long = "password",
        env = "PASSWORD",
        value_name = "PASSWORD",
        default_value = ""
    )]
    password: String,
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
        "import(\"build\") as build;\n\
        build::run(args);",
    )?;
    Ok(())
}
