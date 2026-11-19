//! `.mod/socle.toml` and `.mod/socle.lock`, in the formats of the shared socle
//! ([`mc_core::model::socle`]), with the versions `mod init` pins.

use serde::{Deserialize, Serialize};

pub use mc_core::model::socle::{LockedFile, LockedKind, SocleLock};

/// `.mod/socle.toml`.
pub type SocleConfig = mc_core::model::socle::SocleConfig<Versions>;

/// The versions pinned for the branch's Minecraft version, under `[init]`.
/// The build reads them (`build-logic/`); `update` keeps them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Versions {
    pub minecraft: String,
    pub fabric_loader: String,
    pub fabric_api: String,
    pub loom: String,
    pub neoforge: String,
    pub moddevgradle: String,
    pub modmenu: String,
}
