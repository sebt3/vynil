use std::process::{Command, Stdio, self, Output};
use anyhow::{Result, bail};

pub fn run (command: &String) -> Output {
    Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|_| panic!("The command {:?} failed.", command))
}

pub fn run_log(command: &String) -> Result<()> {
    let output = run(command);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    if !stdout.is_empty() {
        log::info!("{}", stdout.trim());
    }
    if !stderr.is_empty() {
        log::warn!("{}", stderr.trim());
    }
    if ! output.status.success() {
        bail!("The command {:?} failed.", command);
    }
    Ok(())
}

pub fn run_log_check(command: &String) {
    match run_log(command)  {Ok(_) => {}, Err(e) => {
        log::error!("{e}");
        process::exit(1);
    }}
}

pub fn get_output(command: &String) -> Result<String> {
    let output = run(command);
    let stderr = String::from_utf8(output.stderr).unwrap();
    if !stderr.is_empty() {
        log::warn!("{}", stderr.trim());
    }
    if ! output.status.success() {
        bail!("The command {:?} failed.", command);
    }
    let output = String::from_utf8(output.stdout).unwrap();
    Ok(output.trim().to_string())
}
