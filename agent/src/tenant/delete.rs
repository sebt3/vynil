use clap::Args;
use common::{context::set_tenant, instancetenant::TenantInstance, rhaihandler::Script, Result};
use serde::{Deserialize, Serialize};

#[derive(Args, Debug, Serialize, Deserialize)]
pub struct Parameters {
    /// Instance namespace to delete
    #[arg(short = 'n', long = "namespace", env = "NAMESPACE", value_name = "NAMESPACE")]
    namespace: String,
    /// Instance name to delete
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
    /// Configuration directory
    #[arg(
        short = 'c',
        long = "config-dir",
        env = "CONFIG_DIR",
        value_name = "CONFIG_DIR",
        default_value = "."
    )]
    config_dir: String,
}

pub async fn run(args: &Parameters) -> Result<()> {
    let mut rhai = Script::new(vec![
        format!("{}/scripts", args.package_dir),
        format!("{}", args.config_dir),
        format!("{}/tenant", args.script_dir),
        format!("{}/lib", args.script_dir),
    ]);
    let context = TenantInstance::get(args.namespace.clone(), args.instance.clone()).await?;
    set_tenant(context.clone());
    rhai.ctx.set_value("instance", context);
    rhai.set_dynamic("args", &serde_json::to_value(args).unwrap());
    let _ = rhai.eval(
        "import(\"context\") as ctx;\n\
        let context = ctx::run(instance, args);\n\
        import(\"delete\") as delete;\n\
        delete::run(instance, context);",
    )?;
    Ok(())
}
