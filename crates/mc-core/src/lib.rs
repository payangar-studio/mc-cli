//! Code shared by the Minecraft CLIs (`modpack`, and `mod` to come): the
//! error model, prompts, secrets, the platforms, the socle mechanics, mod
//! resolution, the project page model and the dedicated server.
//!
//! Nothing here knows a project type: each binary passes what is its own
//! (its socle directory, its templates, its init values).
//!
//! This file only registers modules, one line each.

pub mod auth;
pub mod error;
pub mod fetch;
pub mod hash;
pub mod machine_config;
pub mod model;
pub mod page;
pub mod platform;
pub mod project;
pub mod prompt;
pub mod resolve;
pub mod secrets;
pub mod server;
pub mod socle;
