use clap::Args;
use common::vynilpackage::VERSION;

#[derive(Args, Debug)]
pub struct Parameters {}

pub fn run(_args: &Parameters) {
    println!("{VERSION}");
}
