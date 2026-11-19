//! `.modpack/socle.toml` and `.modpack/socle.lock`, in the formats of the
//! shared socle ([`mc_core::model::socle`]), with the values `modpack init`
//! records.

use serde::{Deserialize, Serialize};

pub use mc_core::model::socle::{LockedFile, LockedKind, SocleLock, VersionCheck};

/// `.modpack/socle.toml`.
pub type SocleConfig = mc_core::model::socle::SocleConfig<InitValues>;

/// Values given to `modpack init`, kept by every update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct InitValues {
    pub name: String,
    pub minecraft: String,
    pub loader: String,
    pub loader_version: String,
    pub author: String,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::socle::SOCLE;

    fn fixture_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/repo")
    }

    #[test]
    fn config_fixture_round_trips() {
        let text = std::fs::read_to_string(fixture_root().join(SOCLE.config_path())).unwrap();
        let config = SocleConfig::parse(&SOCLE, &text).unwrap();
        assert_eq!(config.init.loader, "neoforge");
        assert_eq!(config.to_toml(&SOCLE), text);
    }

    #[test]
    fn lock_fixture_round_trips_and_matches_the_files() {
        let root = fixture_root();
        let text = std::fs::read_to_string(root.join(SOCLE.lock_path())).unwrap();
        let lock = SocleLock::parse(&SOCLE, &text).unwrap();
        assert_eq!(lock.to_toml(&SOCLE), text);
        let file = lock
            .files
            .iter()
            .find(|f| f.kind == LockedKind::File)
            .unwrap();
        assert_eq!(
            crate::model::sha256_file(&root.join(&file.path)).unwrap(),
            file.sha256
        );
        assert!(lock.files.iter().any(|f| f.kind == LockedKind::Block));
    }

    #[test]
    fn save_then_load() {
        let dir = tempfile::tempdir().unwrap();
        let config = SocleConfig::load(&SOCLE, &fixture_root()).unwrap();
        config.save(&SOCLE, dir.path()).unwrap();
        assert_eq!(SocleConfig::load(&SOCLE, dir.path()).unwrap(), config);
        let lock = SocleLock::load(&SOCLE, &fixture_root()).unwrap();
        lock.save(&SOCLE, dir.path()).unwrap();
        assert_eq!(SocleLock::load(&SOCLE, dir.path()).unwrap(), lock);
    }

    #[test]
    fn cli_version_is_the_crate_version() {
        assert_eq!(SOCLE.cli_version().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
