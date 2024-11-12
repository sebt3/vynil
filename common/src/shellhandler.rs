use crate::{rhai_err, Error, Result, RhaiRes};
use std::process::{Command, Output, Stdio};

pub fn run(command: String) -> Result<Output> {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| Error::Stdio(e))
}

pub fn rhai_run(command: String) -> RhaiRes<i64> {
    let out = run(command).map_err(|e| rhai_err(e))?;
    Ok(i64::from(out.status.code().unwrap_or(0)))
}

pub fn get_out(command: String) -> Result<Output> {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| Error::Stdio(e))
}

pub fn rhai_get_stdout(command: String) -> RhaiRes<String> {
    let out = get_out(command).map_err(|e| rhai_err(e))?;
    if !out.status.success() {
        Err(rhai_err(Error::Other(format!(
            "Command failed, rc={}",
            out.status.code().unwrap_or(-1)
        ))))
    } else if out.stderr.len() > 0 {
        let err = String::from_utf8(out.stderr).map_err(|e| rhai_err(Error::UTF8(e)))?;
        tracing::warn!(err);
        Err(rhai_err(Error::Other(format!("Command had stderr : {}", err))))
    } else {
        let output = String::from_utf8(out.stdout).map_err(|e| rhai_err(Error::UTF8(e)))?;
        Ok(output)
    }
}
// Pour quelque choses de plus complet :
// https://stackoverflow.com/a/72831067
