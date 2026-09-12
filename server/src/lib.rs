#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::arithmetic_side_effects, clippy::indexing_slicing))]

pub mod anonymize;
pub mod auth;
pub mod authz;
pub mod collect;
pub mod config;
pub mod discovery;
pub mod dto;
pub mod error;
pub mod server;
pub mod state;

pub use config::Config;
pub use error::DiagError;
pub use state::AppState;
