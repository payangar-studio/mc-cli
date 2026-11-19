//! The platforms as the modpack CLI uses them: the shared contract
//! ([`mc_core::platform`]), and the operations only a modpack needs, in
//! [`PackPlatform`]: checking that a mod can go into a pack, building the
//! pack, pushing the pack's page.

mod curseforge_pack;
mod mrpack;
mod page_push;

use std::path::{Path, PathBuf};

#[cfg(test)]
pub use mc_core::platform::fake;
pub use mc_core::platform::{
    Candidate, Capability, Download, Jar, Match, Operation, Platform, ProjectName, curseforge,
    modrinth, unsupported,
};

use crate::error::Result;
use crate::model::manifest::{Entry, Pack};
use crate::secrets::{SecretName, Secrets};
use curseforge::CurseForge;
use modrinth::Modrinth;

/// The platforms a modpack command asks, in the order they are tried.
pub type Registry = mc_core::platform::Registry<dyn PackPlatform>;

/// The platforms built into the CLI, in the order they are tried.
pub fn builtin() -> Registry {
    Registry::new(vec![
        Box::new(Modrinth::default()),
        Box::new(CurseForge::new()),
    ])
}

/// Whether a mod can go into this platform's pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Eligibility {
    Eligible,
    /// Blocks the pack for this platform, for the reason given.
    Blocked {
        reason: String,
    },
}

/// What a pack artifact is built from.
#[derive(Debug, Clone)]
pub struct PackBuild<'a> {
    /// Name of the pack.
    pub name: &'a str,
    /// Version of the pack being built.
    pub version: &'a str,
    pub pack: &'a Pack,
    /// The entries of the variant, mods and resource packs.
    pub entries: Vec<&'a Entry>,
    /// Files to put under `overrides/`, in order: a later one replaces an
    /// earlier one at the same path.
    pub overrides: Vec<Override>,
    /// Directory to write the artifact in.
    pub out_dir: &'a Path,
    /// File name of the artifact without extension; the platform adds its own.
    pub file_stem: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    /// File on disk.
    pub source: PathBuf,
    /// Path inside `overrides/`, with `/` separators.
    pub path: String,
}

/// The operations of a platform only a modpack needs. Their capabilities are
/// declared with the others, by [`Platform::capability`]; operations a
/// platform declares `Unsupported` keep the default body, which fails with a
/// clear error.
pub trait PackPlatform: Platform {
    fn pack_eligibility(&self, entry: &Entry) -> Result<Eligibility> {
        let _ = entry;
        Err(unsupported(self.name(), Operation::PackEligibility))
    }

    /// What to check by hand about a file embedded in this platform's pack
    /// (a local entry, under `overrides/<kind dir>/`), if anything.
    fn embedded_jar_note(&self) -> Option<String> {
        None
    }

    /// Builds the pack artifact and returns its path.
    fn build_pack(&self, build: &PackBuild<'_>) -> Result<PathBuf> {
        let _ = build;
        Err(unsupported(self.name(), Operation::BuildPack))
    }

    /// Pushes the page versioned in the repository at `root` to the platform.
    fn push_page(&self, root: &Path) -> Result<()> {
        let _ = root;
        Err(unsupported(self.name(), Operation::PushPage))
    }
}

impl PackPlatform for Modrinth {
    fn pack_eligibility(&self, entry: &Entry) -> Result<Eligibility> {
        Ok(if mrpack::allowed_downloads(entry).is_empty() {
            Eligibility::Blocked {
                reason: format!(
                    "no download URL from {}: the file must be embedded in the pack, with its author's permission",
                    mrpack::ALLOWED_HOSTS.join(", ")
                ),
            }
        } else {
            Eligibility::Eligible
        })
    }

    fn build_pack(&self, build: &PackBuild<'_>) -> Result<PathBuf> {
        mrpack::build(build)
    }

    fn push_page(&self, root: &Path) -> Result<()> {
        let page = page_push::load(root)?;
        let token = Secrets::system().get(SecretName::ModrinthToken)?;
        println!("{}", page_push::push(self.client(), &page, &token)?);
        Ok(())
    }
}

impl PackPlatform for CurseForge {
    fn pack_eligibility(&self, entry: &Entry) -> Result<Eligibility> {
        if curseforge::ids(entry).is_ok() {
            return Ok(Eligibility::Eligible);
        }
        let reason = if self.has_key() {
            "not hosted on CurseForge: a CurseForge pack accepts it only if it is on CurseForge's approved list of third-party mods, which has no API; check it by hand".to_owned()
        } else {
            format!(
                "no CurseForge ids in modpack.lock.toml, and no CurseForge Core API key to look them up: store one with `modpack auth` or set {}, then run `modpack manifest` to fill them",
                curseforge::KEY_VAR
            )
        };
        Ok(Eligibility::Blocked { reason })
    }

    fn embedded_jar_note(&self) -> Option<String> {
        Some("a CurseForge pack accepts an embedded file only if it is on CurseForge's approved list of third-party mods, which has no API; check it by hand".to_owned())
    }

    fn build_pack(&self, build: &PackBuild<'_>) -> Result<PathBuf> {
        curseforge_pack::build(build)
    }
}

#[cfg(test)]
mod fake_pack;
#[cfg(test)]
mod tests;
