use clap::Args;
use common::{context::set_box, jukebox::JukeBox, rhaihandler::Script, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Args, Debug, Serialize, Deserialize)]
pub struct Parameters {
    /// Jukebox name to scan
    #[arg(short = 'j', long = "jukebox", env = "JUKEBOX", value_name = "JUKEBOX")]
    jukebox: String,
    /// Namespace to read secret from
    #[arg(
        short = 'v',
        long = "vynil-namespace",
        env = "VYNIL_NAMESPACE",
        value_name = "VYNIL_NAMESPACE"
    )]
    namespace: String,
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
        format!("{}/boxes", args.script_dir),
        format!("{}/lib", args.script_dir),
    ]);
    let context = JukeBox::get(args.jukebox.clone()).await?;
    set_box(context.clone());
    rhai.ctx.set_value("box", context);
    rhai.set_dynamic("args", &serde_json::to_value(args).unwrap());
    let _ = rhai.run_file(&PathBuf::from(format!("{}/boxes/scan.rhai", args.script_dir)))?;
    Ok(())
}
