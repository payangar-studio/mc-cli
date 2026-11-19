//! `mod.toml`: the mod's identity and contract, owned by the project. `mod init`
//! writes it once; the build reads it (`build-logic/`) to fill the loader
//! metadata.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use mc_core::model::{parse_toml, read_file, to_toml, write_file};

pub const FILE_NAME: &str = "mod.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModToml {
    pub id: String,
    pub name: String,
    pub package: String,
    pub side: Side,
    pub loaders: Vec<Loader>,
    pub license: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,
}

/// Where the mod runs. It sets the loader metadata, the publication's sides
/// and the tests the socle lays down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    /// Client only.
    Client,
    /// Dedicated server only.
    Server,
    /// Both sides, both required.
    BothRequired,
    /// Both sides, one of them optional.
    BothOptional,
}

impl Side {
    pub const ALL: [Side; 4] = [
        Side::Client,
        Side::Server,
        Side::BothRequired,
        Side::BothOptional,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Side::Client => "client",
            Side::Server => "server",
            Side::BothRequired => "both-required",
            Side::BothOptional => "both-optional",
        }
    }

    /// Whether the mod has client code.
    pub fn has_client(self) -> bool {
        self != Side::Server
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A loader the socle builds for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Fabric,
    Neoforge,
}

impl Loader {
    pub const ALL: [Loader; 2] = [Loader::Fabric, Loader::Neoforge];

    /// Name in `mod.toml`, on the command line, and of its subproject.
    pub fn as_str(self) -> &'static str {
        match self {
            Loader::Fabric => "fabric",
            Loader::Neoforge => "neoforge",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.as_str() == name)
    }
}

impl fmt::Display for Loader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A dependency, declared once for the loaders and for the publication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    /// Mod id on the loaders.
    pub id: String,
    /// `required`, `optional` or `incompatible`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Loaders it applies to; all the mod's loaders when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loaders: Option<Vec<Loader>>,
    /// Modrinth project slug.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modrinth: Option<String>,
    /// CurseForge project slug.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curseforge: Option<String>,
}

impl ModToml {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(FILE_NAME);
        parse_toml(
            &read_file(&path)?,
            FILE_NAME,
            "fix the file; its format is described in its header",
        )
    }

    pub fn to_toml(&self) -> String {
        format!(
            "# The mod's identity and contract. The build fills fabric.mod.json and\n\
             # neoforge.mods.toml from it: side (client, server, both-required,\n\
             # both-optional), loaders, license and dependencies. A dependency is\n\
             # declared once, for the loaders and for the publication:\n\
             #   [[dependencies]]\n\
             #   id = \"sodium\"          # mod id on the loaders\n\
             #   type = \"optional\"      # required, optional or incompatible\n\
             #   loaders = [\"fabric\"]   # all the mod's loaders when absent\n\
             #   modrinth = \"sodium\"    # project slug on each platform\n\
             #   curseforge = \"sodium\"\n\n{}",
            to_toml(self)
        )
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        write_file(&root.join(FILE_NAME), &self.to_toml())
    }
}

/// Checks a mod id: what both loaders accept, `[a-z][a-z0-9_]{1,63}`.
pub fn check_id(id: &str) -> Result<()> {
    let mut chars = id.chars();
    let valid = (2..=64).contains(&id.len())
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(Error::usage(
            format!("`{id}` is not a valid mod id"),
            "pass --id with 2 to 64 characters: a lowercase letter, then lowercase letters, digits or `_`",
        ))
    }
}

/// Words Java reserves, which no package segment can be: the keywords, the
/// literals, and `_`.
const JAVA_RESERVED: &[&str] = &[
    "_",
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "false",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "true",
    "try",
    "void",
    "volatile",
    "while",
];

/// Checks a Java package: lowercase segments separated by dots, none of them
/// a word Java reserves.
pub fn check_package(package: &str) -> Result<()> {
    let segment = |s: &str| {
        let mut chars = s.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            && !JAVA_RESERVED.contains(&s)
    };
    if package.split('.').all(segment) {
        Ok(())
    } else {
        Err(Error::usage(
            format!("`{package}` is not a valid Java package"),
            "pass --package with lowercase segments separated by dots, none of them a Java keyword, such as `com.payangar.swiftblades`",
        ))
    }
}

/// The package of a mod under the base package `base`: `com.payangar` and
/// `swift_blades` give `com.payangar.swiftblades`.
pub fn default_package(base: &str, id: &str) -> String {
    format!("{base}.{}", id.replace('_', ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> ModToml {
        ModToml {
            id: "swift_blades".into(),
            name: "Swift Blades".into(),
            package: "com.payangar.swiftblades".into(),
            side: Side::BothRequired,
            loaders: vec![Loader::Fabric, Loader::Neoforge],
            license: "MIT".into(),
            dependencies: vec![Dependency {
                id: "sodium".into(),
                kind: "optional".into(),
                loaders: Some(vec![Loader::Fabric]),
                modrinth: Some("sodium".into()),
                curseforge: Some("sodium".into()),
            }],
        }
    }

    #[test]
    fn round_trips_in_the_design_format() {
        let dir = tempfile::tempdir().unwrap();
        example().save(dir.path()).unwrap();
        let text = std::fs::read_to_string(dir.path().join(FILE_NAME)).unwrap();
        assert!(text.contains("side = \"both-required\""), "{text}");
        assert!(
            text.contains("loaders = [\"fabric\", \"neoforge\"]"),
            "{text}"
        );
        assert!(
            text.contains("[[dependencies]]\nid = \"sodium\"\ntype = \"optional\""),
            "{text}"
        );
        assert_eq!(ModToml::load(dir.path()).unwrap(), example());
    }

    #[test]
    fn ids_follow_the_loaders_rules() {
        for id in ["swift_blades", "ab", "a1"] {
            check_id(id).unwrap();
        }
        for id in ["a", "Swift", "1abc", "swift-blades", "_x", &"a".repeat(65)] {
            assert!(check_id(id).is_err(), "{id}");
        }
    }

    #[test]
    fn packages_are_lowercase_segments() {
        check_package("com.payangar.swiftblades").unwrap();
        check_package("com.payangar._x.var").unwrap();
        for package in [
            "",
            "com.",
            "Com.x",
            "com.1x",
            "com..x",
            "com.payangar.native",
            "com._",
            "true.x",
        ] {
            assert!(check_package(package).is_err(), "{package}");
        }
    }

    #[test]
    fn default_package_drops_underscores() {
        assert_eq!(
            default_package("com.payangar", "swift_blades"),
            "com.payangar.swiftblades"
        );
    }
}
