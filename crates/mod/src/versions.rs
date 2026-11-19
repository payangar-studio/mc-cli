//! The versions the socle pins for each Minecraft version it supports: the
//! loaders, Fabric API and the Gradle plugins. `mod init` copies the set of
//! its Minecraft version into `.mod/socle.toml`, which the build reads.
//!
//! Each set is read from the official maven metadata when it is added, never
//! guessed: maven.fabricmc.net (Loom, Fabric Loader, Fabric API),
//! maven.neoforged.net (NeoForge, ModDevGradle) and maven.terraformersmc.com
//! (Mod Menu).

use crate::model::socle::Versions;

/// One supported Minecraft version and what goes with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pinned {
    pub minecraft: &'static str,
    pub fabric_loader: &'static str,
    pub fabric_api: &'static str,
    /// Loom, as the `net.fabricmc.fabric-loom` plugin (no remapping).
    pub loom: &'static str,
    pub neoforge: &'static str,
    /// ModDevGradle, as the `net.neoforged.moddev` plugin.
    pub moddevgradle: &'static str,
    /// Mod Menu, compile-only: the shared library's config screen entrypoint.
    pub modmenu: &'static str,
}

/// Supported Minecraft versions, the latest first: it is the default.
pub const SUPPORTED: &[Pinned] = &[Pinned {
    minecraft: "26.3",
    fabric_loader: "0.20.1",
    fabric_api: "0.166.2+26.3",
    loom: "1.19.4",
    neoforge: "26.3.0.113",
    moddevgradle: "2.0.148",
    modmenu: "21.0.1",
}];

/// The pinned set of `minecraft`, if supported.
pub fn of(minecraft: &str) -> Option<&'static Pinned> {
    SUPPORTED.iter().find(|p| p.minecraft == minecraft)
}

impl Pinned {
    /// The values `.mod/socle.toml` records.
    pub fn versions(&self) -> Versions {
        Versions {
            minecraft: self.minecraft.into(),
            fabric_loader: self.fabric_loader.into(),
            fabric_api: self.fabric_api.into(),
            loom: self.loom.into(),
            neoforge: self.neoforge.into(),
            moddevgradle: self.moddevgradle.into(),
            modmenu: self.modmenu.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabric_api_matches_its_minecraft_version() {
        for pinned in SUPPORTED {
            let suffix = format!("+{}", pinned.minecraft);
            assert!(pinned.fabric_api.ends_with(&suffix), "{pinned:?}");
            assert!(
                pinned
                    .neoforge
                    .starts_with(&format!("{}.", pinned.minecraft)),
                "{pinned:?}"
            );
        }
    }

    #[test]
    fn unknown_version_has_no_set() {
        assert_eq!(of("26.3").unwrap().minecraft, "26.3");
        assert!(of("1.21.1").is_none());
    }
}
