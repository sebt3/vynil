use clap::Args;
use common::{Result, rhaihandler::Script};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Args, Debug, Serialize, Deserialize)]
pub struct Parameters {
    /// Instance namespace to backup
    #[arg(short = 'n', long = "namespace", env = "NAMESPACE", value_name = "NAMESPACE")]
    namespace: String,
    /// Instance name to backup
    #[arg(short = 'i', long = "instance", env = "INSTANCE", value_name = "INSTANCE")]
    instance: String,
    /// Vynil namespace
    #[arg(
        short = 'v',
        long = "vynil-namespace",
        env = "VYNIL_NAMESPACE",
        value_name = "VYNIL_NAMESPACE"
    )]
    vynil_namespace: String,
    /// Package directory
    #[arg(
        short = 'p',
        long = "package-dir",
        env = "PACKAGE_DIRECTORY",
        value_name = "PACKAGE_DIRECTORY",
        default_value = "/tmp/package"
    )]
    package_dir: PathBuf,
    /// Agent script directory
    #[arg(
        short = 's',
        long = "script-dir",
        env = "SCRIPT_DIRECTORY",
        value_name = "SCRIPT_DIRECTORY",
        default_value = "./agent/scripts"
    )]
    script_dir: PathBuf,
    /// Agent template directory
    #[arg(
        short = 't',
        long = "template-dir",
        env = "TEMPLATE_DIRECTORY",
        value_name = "TEMPLATE_DIRECTORY",
        default_value = "./agent/templates"
    )]
    template_dir: PathBuf,
    /// Agent image
    #[arg(
        long = "agent-image",
        env = "AGENT_IMAGE",
        value_name = "AGENT_IMAGE",
        default_value = common::DEFAULT_AGENT_IMAGE
    )]
    agent_image: String,
    /// version
    #[arg(long = "tag", env = "TAG", value_name = "TAG")]
    tag: String,
    /// Configuration directory
    #[arg(
        short = 'c',
        long = "config-dir",
        env = "CONFIG_DIR",
        value_name = "CONFIG_DIR",
        default_value = "."
    )]
    config_dir: PathBuf,
    /// Controller computed values
    #[arg(
        long = "controller-values",
        env = "CONTROLLER_VALUES",
        value_name = "CONTROLLER_VALUES",
        default_value = "{}"
    )]
    controller_values: String,
}

pub async fn run(args: &Parameters) -> Result<()> {
    let mut rhai = Script::new(vec![
        format!("{}/scripts", args.package_dir.display()),
        format!("{}", args.config_dir.display()),
        format!("{}/tenant", args.script_dir.display()),
        format!("{}/lib", args.script_dir.display()),
    ]);
    rhai.set_dynamic("args", &serde_json::to_value(args).unwrap());
    let _ = rhai.eval(
        "import(\"backup\") as backup;\n\
        backup::run(args);",
    )?;
    Ok(())
}
