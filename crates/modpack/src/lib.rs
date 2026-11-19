//! Library behind the `modpack` binary, built on the shared crate `mc-core`.
//!
//! This file only registers modules. New modules are added here, one line
//! each; merge conflicts on these lines are resolved by keeping both.

pub mod cli;
pub mod conditions;
pub mod context;
pub use mc_core::error;
pub mod launcher;
pub mod model;
pub mod ops;
pub mod pack;
pub mod platform;
pub use mc_core::prompt;
pub use mc_core::secrets;
pub mod setup;
pub mod socle;
pub mod status;
pub mod store;
pub mod variant;
