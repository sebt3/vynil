#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::arithmetic_side_effects, clippy::indexing_slicing))]

pub mod actions;
pub mod bundle;
pub mod cli;
pub mod completion;
pub mod items;
pub mod transport;
