use clap::Args;
use common::{Result, context::set_service, instanceservice::ServiceInstance, rhaihandler::Script};
use serde::{Deserialize, Serialize};

#[derive(Args, Debug, Serialize, Deserialize)]
pub struct Parameters {
    /// Instance namespace to install
    #[arg(short = 'n', long = "namespace", env = "NAMESPACE", value_name = "NAMESPACE")]
    namespace: String,
    /// Instance name to install
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
    package_dir: String,
    /// Agent script directory
    #[arg(
        short = 's',
        long = "script-dir",
        env = "SCRIPT_DIRECTORY",
        value_name = "SCRIPT_DIRECTORY",
        default_value = "./agent/scripts"
    )]
    script_dir: String,
    /// Agent template directory
    #[arg(
        short = 't',
        long = "template-dir",
        env = "TEMPLATE_DIRECTORY",
        value_name = "TEMPLATE_DIRECTORY",
        default_value = "./agent/templates"
    )]
    template_dir: String,
    /// Agent image
    #[arg(
        long = "agent-image",
        env = "AGENT_IMAGE",
        value_name = "AGENT_IMAGE",
        default_value = "docker.io/sebt3/vynil-agent:0.5.8"
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
    config_dir: String,
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
        format!("{}/scripts", args.package_dir),
        format!("{}", args.config_dir),
        format!("{}/service", args.script_dir),
        format!("{}/lib", args.script_dir),
    ]);
    let context = ServiceInstance::get(args.namespace.clone(), args.instance.clone()).await?;
    set_service(context.clone());
    rhai.ctx.set_value("instance", context);
    rhai.set_dynamic("args", &serde_json::to_value(args).unwrap());
    let _ = rhai.eval(
        "import(\"context\") as ctx;\n\
        let context = ctx::run(instance, args);\n\
        import(\"install\") as install;\n\
        install::run(instance, context);",
    )?;
    Ok(())
}
