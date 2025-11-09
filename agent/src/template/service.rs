use super::Contexts;
use clap::Args;
use common::{Result, rhaihandler::Script};
use serde::{Deserialize, Serialize};

#[derive(Args, Debug, Serialize, Deserialize)]
pub struct Parameters {
    /// Instance namespace to install
    #[arg(
        short = 'n',
        long = "namespace",
        env = "NAMESPACE",
        value_name = "NAMESPACE",
        default_value = "default"
    )]
    namespace: String,
    /// Instance name to install
    #[arg(
        short = 'i',
        long = "instance",
        env = "INSTANCE",
        value_name = "INSTANCE",
        default_value = "instance-name"
    )]
    instance: String,
    /// Vynil namespace
    #[arg(
        short = 'v',
        long = "vynil-namespace",
        env = "VYNIL_NAMESPACE",
        value_name = "VYNIL_NAMESPACE",
        default_value = "vynil-system"
    )]
    vynil_namespace: String,
    /// Package directory
    #[arg(
        short = 'o',
        long = "source",
        env = "SOURCE",
        value_name = "SOURCE",
        default_value = "/src"
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
        default_value = "docker.io/sebt3/vynil-agent:0.5.7"
    )]
    agent_image: String,
    /// version
    #[arg(long = "tag", env = "TAG", value_name = "TAG", default_value = "1.0.0")]
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
    /// Use context
    #[arg(
        long = "context",
        env = "CONTEXT_NAME",
        value_name = "CONTEXT_NAME",
        default_value_t,
        value_enum
    )]
    context_name: Contexts,
}

pub async fn run(args: &Parameters) -> Result<()> {
    let mut rhai = Script::new(vec![
        format!("{}/scripts", args.package_dir),
        format!("{}", args.config_dir),
        format!("{}/service", args.script_dir),
        format!("{}/lib", args.script_dir),
    ]);
    rhai.set_dynamic("args", &serde_json::to_value(args).unwrap());
    let _ = rhai.eval(
        "import(\"context\") as ctx;\n\
        let instance = #{\n\
            metadata: #{\n\
                name: args.instance,\n\
                namespace: args.namespace\n\
            },\n\
        };\n\
        let context = ctx::template(instance, args);\n\
        import(\"template\") as template;\n\
        template::run(instance, context);",
    )?;
    Ok(())
}
