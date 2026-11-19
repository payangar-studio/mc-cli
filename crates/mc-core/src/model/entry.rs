//! An entry: one mod or resource pack, with its file, its hashes, the sides
//! it runs on and one free table per platform. A modpack's manifest lists
//! entries; mod resolution builds them.
//!
//! Each entry is a superset of a `files[]` entry of a `.mrpack` (`path`,
//! `hashes`, `fileSize`, `env`, `downloads`), plus the stable `key` that
//! names it, its `type`, an optional direct `url`, the `source` of a local
//! file and its approval, and one free table per platform. Only the platform
//! named by a table reads it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What a pack or a test profile runs on, after the `dependencies` of a
/// `.mrpack`: the platforms resolve versions for it, the server is installed
/// for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Pack {
    /// Minecraft version, such as `1.21.1`.
    pub minecraft: String,
    /// Loader, as Modrinth names it: `neoforge`, `fabric`, `forge`, `quilt`.
    pub loader: String,
    pub loader_version: String,
}

/// One mod or resource pack. Fields serialize in declaration order: plain
/// values first, then tables, as TOML requires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Stable identifier of the entry, which the project's other files refer
    /// to (`variants.toml` of a modpack). Never changes.
    pub key: String,
    #[serde(rename = "type")]
    pub kind: Kind,
    /// Path of the file relative to the pack root, with `/` separators: under
    /// the kind's directory, or under its local directory for a local file.
    pub path: String,
    #[serde(rename = "fileSize")]
    pub file_size: u64,
    /// Download URLs, as in a `.mrpack`. Empty when no platform hosts a
    /// redistributable copy.
    #[serde(default)]
    pub downloads: Vec<String>,
    /// Direct URL, last resort for a file hosted on no platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// `local` for a file tracked by git in the kind's local directory, at
    /// `path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// sha512 of the local file `modpack approve` approved for publication.
    /// The approval holds while it equals `hashes.sha512`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<String>,
    pub hashes: Hashes,
    #[serde(default)]
    pub env: Env,
    /// One table per platform, named after it. Read and written only by that
    /// platform's implementation.
    #[serde(flatten)]
    pub platforms: BTreeMap<String, toml::Table>,
}

/// What an entry is, which sets where its file goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mod,
    Resourcepack,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Mod, Kind::Resourcepack];

    /// Directory of its files, in the repository (downloaded, ignored by git)
    /// and in an instance or a pack.
    pub fn dir(self) -> &'static str {
        match self {
            Kind::Mod => "mods",
            Kind::Resourcepack => "resourcepacks",
        }
    }

    /// Directory of its local files in the repository, tracked by git.
    pub fn local_dir(self) -> &'static str {
        match self {
            Kind::Mod => "mods-local",
            Kind::Resourcepack => "resourcepacks-local",
        }
    }

    /// Extension of its files, lowercase, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Kind::Mod => "jar",
            Kind::Resourcepack => "zip",
        }
    }

    /// The sides an entry of this kind runs on until a platform says
    /// otherwise: both for a mod, as in a `.mrpack`; the client only for a
    /// resource pack, which a server never loads.
    pub fn default_env(self) -> Env {
        match self {
            Kind::Mod => Env::default(),
            Kind::Resourcepack => Env {
                client: Support::Required,
                server: Support::Unsupported,
            },
        }
    }

    /// How messages name it.
    pub fn noun(self) -> &'static str {
        match self {
            Kind::Mod => "mod",
            Kind::Resourcepack => "resource pack",
        }
    }

    /// The kind whose files have the extension of `name`, in any case.
    pub fn of_file_name(name: &str) -> Option<Kind> {
        let (_, ext) = name.rsplit_once('.')?;
        Kind::ALL
            .into_iter()
            .find(|k| ext.eq_ignore_ascii_case(k.extension()))
    }

    /// The kind of a repository path, and whether the path is under the
    /// kind's local directory: `resourcepacks-local/x.zip` gives
    /// `(Resourcepack, true)`.
    pub fn of_path(path: &str) -> Option<(Kind, bool)> {
        let (dir, _) = path.split_once('/')?;
        Kind::ALL.into_iter().find_map(|k| {
            if dir == k.dir() {
                Some((k, false))
            } else if dir == k.local_dir() {
                Some((k, true))
            } else {
                None
            }
        })
    }
}

/// Where the file of an entry no platform hosts comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Tracked by git in the repository, under the kind's local directory.
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hashes {
    pub sha1: String,
    pub sha512: String,
}

/// Which sides need the entry. A missing `env` means required on both, as in a
/// `.mrpack`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Env {
    pub client: Support,
    pub server: Support,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Support {
    #[default]
    Required,
    Optional,
    Unsupported,
}

impl Entry {
    /// The table of a platform, if the entry has one.
    pub fn platform(&self, name: &str) -> Option<&toml::Table> {
        self.platforms.get(name)
    }

    /// The table of a platform, created empty if missing.
    pub fn platform_mut(&mut self, name: &str) -> &mut toml::Table {
        self.platforms.entry(name.to_owned()).or_default()
    }

    /// Whether the file is tracked by git in the kind's local directory.
    pub fn is_local(&self) -> bool {
        self.source == Some(Source::Local)
    }

    /// Whether a local entry's approval matches its file as the manifest
    /// knows it.
    pub fn is_approved(&self) -> bool {
        self.approved.as_deref() == Some(self.hashes.sha512.as_str())
    }

    /// File name of the file.
    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Where the file goes in an instance or a pack, `<kind dir>/<file name>`
    /// (`mods/<file name>` for a mod), for a local entry as for any other.
    pub fn instance_path(&self) -> String {
        format!("{}/{}", self.kind.dir(), self.file_name())
    }

    /// How messages name the entry: its kind's noun.
    pub fn noun(&self) -> &'static str {
        self.kind.noun()
    }
}
