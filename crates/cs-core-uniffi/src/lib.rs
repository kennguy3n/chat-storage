#![allow(clippy::too_many_arguments)]

uniffi::setup_scaffolding!();

mod error;
mod facade;
mod types;

pub use error::*;
pub use facade::*;
pub use types::*;
