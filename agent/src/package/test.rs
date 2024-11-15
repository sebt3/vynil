use clap::Args;
use common::{
    context::{set_system, set_tenant},
    instancesystem::SystemInstance,
    instancetenant::TenantInstance,
    rhaihandler::Script,
    Result,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(clap::ValueEnum, Clone, Default, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PackageType {
    /// Tenant package type
    Tenant,
    #[default]
    /// System package type
    System,
}

#[derive(Args, Debug, Serialize, Deserialize)]
pub struct Parameters {
    /// Instance namespace to install
    #[arg(short = 'n', long = "namespace", env = "NAMESPACE", value_name = "NAMESPACE")]
    namespace: String,
    /// Instance name to install
    #[arg(short = 'i', long = "instance", env = "INSTANCE", value_name = "INSTANCE")]
    instance: String,
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
    /// package type
    #[arg(
        short = 'p',
        long = "package-type",
        env = "PACKAGE_TYPE",
        value_name = "PACKAGE_TYPE",
        default_value = "system"
    )]
    package_type: PackageType,
    /// Agent script directory
    #[arg(
        short = 's',
        long = "script-dir",
        env = "SCRIPT_DIRECTORY",
        value_name = "SCRIPT_DIRECTORY",
        default_value = "./agent/scripts"
    )]
    script_dir: String,
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
    let mut path = vec![format!("{}/scripts", args.source.to_string_lossy())];
    if args.package_type == PackageType::System {
        path.push(format!("{}/system", args.script_dir));
    } else {
        path.push(format!("{}/tenant", args.script_dir));
    }
    path.push(format!("{}/packages", args.script_dir));
    path.push(format!("{}/lib", args.script_dir));
    let mut rhai = Script::new(path);
    rhai.set_dynamic("args", &serde_json::to_value(args).unwrap());
    if args.package_type == PackageType::System {
        let context = SystemInstance::get(args.namespace.clone(), args.instance.clone()).await?;
        set_system(context.clone());
        rhai.ctx.set_value("instance", context);
    } else {
        let context = TenantInstance::get(args.namespace.clone(), args.instance.clone()).await?;
        set_tenant(context.clone());
        rhai.ctx.set_value("instance", context);
    }
    let _ = rhai.eval(
        "import(\"test\") as test;\n\
        test::run(instance, args);",
    )?;
    Ok(())
}
