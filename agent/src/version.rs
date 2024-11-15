use clap::Args;
use common::{Result, vynilpackage::VERSION};

#[derive(Args, Debug)]
pub struct Parameters {}

pub async fn run(_args: &Parameters) -> Result<()> {
    println!("{}",VERSION);
    Ok(())
}
