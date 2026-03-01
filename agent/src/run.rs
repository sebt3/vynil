use clap::Args;
use common::{Error, Result, rhaihandler::Script};
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct Parameters {
    /// File to run
    #[arg(short = 'f', long = "file", env = "SCRIPT", value_name = "SCRIPT")]
    script: PathBuf,
    /// Agent script directory
    #[arg(
        short = 's',
        long = "script-dir",
        env = "SCRIPT_DIRECTORY",
        value_name = "SCRIPT_DIRECTORY",
        default_value = "./agent/scripts"
    )]
    script_dir: PathBuf,
}

pub async fn run(args: &Parameters) -> Result<()> {
    // Validate that the script parameter is a file
    if !Path::new(&args.script).is_file() {
        tracing::error!("{:?} is not a file", &args.script);
        Err(Error::MissingScript(args.script.clone()))
    } else {
        let p = args.script.as_path().parent().unwrap();
        let mut rhai = Script::new(vec![
            p.to_string_lossy().to_string(),
            format!("{}/lib", args.script_dir.display()),
        ]);
        let _ = rhai.run_file(&args.script)?;
        Ok(())
    }
}
