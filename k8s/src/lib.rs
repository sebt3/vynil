pub mod install;
pub mod distrib;
pub mod events;
pub use anyhow::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;
