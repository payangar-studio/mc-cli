//! The contract every platform implements, and the registry the CLIs ask.
//!
//! A CLI never names a platform: it iterates over the registry, or looks a
//! platform up by a name it read somewhere (a manifest table, `--platform`).
//! Each binary builds its own registry of the platforms it uses
//! (`modpack::platform::builtin` for the modpack CLI): adding a platform is
//! adding an implementation and one line in each binary's list.
//!
//! Each platform owns the table named after it in every entry
//! ([`Entry::platforms`]); nobody else reads or writes it.
//!
//! The operations only one project type needs (building a modpack) are
//! declared by that type's binary, in a trait built on
//! [`Platform`], over a registry of that trait: [`Registry`] takes the trait
//! object it holds as a parameter.

use std::fmt;
use std::io::Read;
use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::model::entry::{Entry, Kind, Pack};
use crate::page::{Listing, Page};

pub mod curseforge;
#[cfg(any(test, feature = "fake"))]
pub mod fake;
pub mod github;
pub mod modrinth;

/// The operations of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Operation {
    Identify,
    Resolve,
    FillEntry,
    Download,
    PackEligibility,
    BuildPack,
    PushPage,
}

impl Operation {
    pub const ALL: [Operation; 7] = [
        Operation::Identify,
        Operation::Resolve,
        Operation::FillEntry,
        Operation::Download,
        Operation::PackEligibility,
        Operation::BuildPack,
        Operation::PushPage,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Operation::Identify => "identify",
            Operation::Resolve => "resolve",
            Operation::FillEntry => "fill_entry",
            Operation::Download => "download",
            Operation::PackEligibility => "pack_eligibility",
            Operation::BuildPack => "build_pack",
            Operation::PushPage => "push_page",
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// How a platform carries out an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    /// By code calling the platform's API.
    Api,
    /// By the named skill, which an agent runs itself (in a browser for
    /// instance). The CLI only tells which skill to run.
    Skill(&'static str),
    Unsupported,
}

/// A jar to identify, with what the core computed from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jar {
    pub path: PathBuf,
    pub sha1: String,
    pub sha512: String,
    pub size: u64,
}

/// A jar a platform recognized.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    /// The project's slug on the platform, used to build the manifest key.
    pub slug: Option<String>,
    /// The platform's table for the manifest entry.
    pub section: toml::Table,
    /// False when the platform recognized the jar only partly (a CurseForge
    /// fingerprint close to, but not equal to, a known file): never to be
    /// taken as an exact match without asking.
    pub exact: bool,
}

/// A version of a mod or resource pack the user asked for, compatible with
/// the pack.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// What to show when asking which version to take.
    pub label: String,
    pub slug: Option<String>,
    /// What the project is, which sets where its file goes.
    pub kind: Kind,
    /// File name of the file, used as its name in its kind's directory.
    pub file_name: String,
    /// The platform's table for the manifest entry.
    pub section: toml::Table,
}

/// How a project shows to people: its slug and its title.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectName {
    pub slug: String,
    pub title: String,
}

/// Where to get a jar from.
pub enum Download {
    Url(String),
    Stream(Box<dyn Read + Send>),
}

impl fmt::Debug for Download {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Download::Url(url) => f.debug_tuple("Url").field(url).finish(),
            Download::Stream(_) => f.write_str("Stream(..)"),
        }
    }
}

/// The contract of a platform. Operations a platform declares `Unsupported`
/// keep the default body, which fails with a clear error. [`Operation`] lists
/// every operation, those of the traits built on this one included, so that a
/// platform declares all its capabilities in one place.
pub trait Platform: Send + Sync {
    /// Name of the platform, also the name of its manifest table.
    fn name(&self) -> &str;

    fn capability(&self, op: Operation) -> Capability;

    /// Recognizes jars by their hashes or fingerprint. The result has one item
    /// per jar, in order; `None` for a jar the platform does not know.
    fn identify(&self, jars: &[Jar]) -> Result<Vec<Option<Match>>> {
        let _ = jars;
        Err(unsupported(self.name(), Operation::Identify))
    }

    /// Versions of the mod or resource pack `query` (slug, id or URL)
    /// compatible with `pack`, best first. Empty when the platform does not
    /// know the project. A project of another type (a shader, a modpack) is a
    /// usage error.
    fn resolve(&self, query: &str, pack: &Pack) -> Result<Vec<Candidate>> {
        let _ = (query, pack);
        Err(unsupported(self.name(), Operation::Resolve))
    }

    /// Slug and title of the project with the id `id`, to name it to people
    /// rather than by its id. `None` when the platform does not know it or
    /// cannot say.
    fn project_name(&self, id: &str) -> Result<Option<ProjectName>> {
        let _ = id;
        Ok(None)
    }

    /// Completes an entry from its table for this platform: download URLs,
    /// sides, required dependencies (kept in the platform table).
    fn fill_entry(&self, entry: &mut Entry) -> Result<()> {
        let _ = entry;
        Err(unsupported(self.name(), Operation::FillEntry))
    }

    /// Where to get the entry's jar.
    fn download(&self, entry: &Entry) -> Result<Download> {
        let _ = entry;
        Err(unsupported(self.name(), Operation::Download))
    }

    /// Pushes `page` to the platform, creating the project as `listing`
    /// says when it has none yet, and returns a line saying what was done.
    fn push_page(&self, page: &Page, listing: &Listing) -> Result<String> {
        let _ = (page, listing);
        Err(unsupported(self.name(), Operation::PushPage))
    }
}

/// Error of an operation a platform does not carry out by code.
pub fn unsupported(platform: &str, op: Operation) -> Error {
    Error::usage(
        format!("platform `{platform}` does not support `{op}`"),
        "leave this platform out of the command (`--platform`)",
    )
}

/// The known platforms, as trait objects of `P`: [`Platform`] itself, or a
/// trait of a project type built on it.
pub struct Registry<P: ?Sized = dyn Platform> {
    platforms: Vec<Box<P>>,
}

impl<P: ?Sized + Platform> Registry<P> {
    pub fn new(platforms: Vec<Box<P>>) -> Self {
        Self { platforms }
    }

    pub fn all(&self) -> impl Iterator<Item = &P> {
        self.platforms.iter().map(Box::as_ref)
    }

    pub fn names(&self) -> Vec<&str> {
        self.all().map(|p| p.name()).collect()
    }

    pub fn find(&self, name: &str) -> Option<&P> {
        self.all().find(|p| p.name() == name)
    }

    /// Like [`Registry::find`], for a name the user typed.
    pub fn get(&self, name: &str) -> Result<&P> {
        self.find(name).ok_or_else(|| {
            Error::usage(
                format!("unknown platform `{name}`"),
                format!("use one of: {}", self.names().join(", ")),
            )
        })
    }
}

impl Registry {
    /// Every operation of a platform with its capability.
    pub fn capabilities(platform: &dyn Platform) -> Vec<(Operation, Capability)> {
        Operation::ALL
            .into_iter()
            .map(|op| (op, platform.capability(op)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakePlatform;
    use super::*;
    use crate::error::ErrorKind;

    fn registry() -> Registry {
        Registry::new(vec![
            Box::new(FakePlatform::new("fake")),
            Box::new(FakePlatform::new("other")),
        ])
    }

    #[test]
    fn registry_finds_a_platform_by_name() {
        let registry = registry();
        assert_eq!(registry.names(), ["fake", "other"]);
        assert_eq!(registry.find("other").unwrap().name(), "other");
        assert!(registry.find("nope").is_none());
    }

    #[test]
    fn unknown_platform_lists_the_known_ones() {
        let err = registry().get("nope").err().unwrap();
        assert_eq!(err.kind(), ErrorKind::Usage);
        assert_eq!(err.hint(), "use one of: fake, other");
    }

    #[test]
    fn registry_exposes_capabilities() {
        let registry = registry();
        let caps = Registry::capabilities(registry.find("fake").unwrap());
        assert_eq!(caps.len(), Operation::ALL.len());
        assert!(caps.contains(&(Operation::Identify, Capability::Api)));
        assert!(caps.contains(&(Operation::PushPage, Capability::Skill("modpack-fake-page"))));
    }

    /// A platform that implements nothing.
    struct Bare;

    impl Platform for Bare {
        fn name(&self) -> &str {
            "bare"
        }
        fn capability(&self, _: Operation) -> Capability {
            Capability::Unsupported
        }
    }

    #[test]
    fn unimplemented_operations_fail_with_a_clear_error() {
        let pack = Pack {
            minecraft: "1.21.1".into(),
            loader: "fabric".into(),
            loader_version: "0.16.0".into(),
        };
        let err = Bare.resolve("sodium", &pack).unwrap_err();
        assert_eq!(
            err.to_string().lines().next().unwrap(),
            "error: platform `bare` does not support `resolve`"
        );
        assert!(Bare.identify(&[]).is_err());
    }
}
