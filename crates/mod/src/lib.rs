//! Library behind the `mod` binary, built on the shared crate `mc-core`.
//!
//! This file only registers modules. New modules are added here, one line
//! each; merge conflicts on these lines are resolved by keeping both.

pub mod cli;
pub mod context;
pub use mc_core::error;
pub mod model;
pub use mc_core::prompt;
pub mod github;
pub mod sides;
pub mod run;
pub mod socle;
pub mod versions;
