use clap::Args;
use common::{vynilpackage::VERSION, Result};

#[derive(Args, Debug)]
pub struct Parameters {}

pub async fn run(_args: &Parameters) -> Result<()> {
    println!("{}", VERSION);
    Ok(())
}
