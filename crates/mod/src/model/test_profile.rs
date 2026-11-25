//! `test-profiles/<name>.toml`: a named set of extra mods, resource packs and
//! game options to run the mod with, versioned with the mod. Its mods and
//! resource packs come from Modrinth, from GitHub releases, or from
//! `test-profiles/local/`, tracked by git, for a file hosted nowhere. `mod run
//! --profile <name>` runs it in `runs/<name>/`, ignored by git. A profile with
//! `ci = true` also runs in the mod's CI, as Fabric gametests with its mods
//! (`.github/workflows/mod-tests.yml`).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::model::mod_toml::Loader;
use mc_core::model::entry::Kind;
use mc_core::model::{parse_toml, read_file};
use mc_core::platform::github;

/// Directory of the profiles, at the repository root.
pub const DIR: &str = "test-profiles";
/// Directory of the local files the profiles name, inside [`DIR`].
pub const LOCAL: &str = "local";
/// The game option of the graphics preset.
const GRAPHICS_PRESET: &str = "graphicsPreset";

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestProfile {
    /// The loader it runs on; when absent, Fabric for a CI profile, else the
    /// mod's first loader.
    #[serde(default)]
    pub loader: Option<Loader>,
    /// Whether the mod's CI runs the Fabric gametests with the profile's mods.
    #[serde(default)]
    pub ci: bool,
    /// Mods: Modrinth ones as `<slug>@<version>`, GitHub ones as
    /// `github:<owner>/<repo>@<tag>[/<file>]` (both pinned), local ones as
    /// `local/<file>`.
    #[serde(default)]
    pub mods: Vec<String>,
    /// Resource packs: Modrinth ones as `<slug>` or `<slug>@<version>`,
    /// GitHub ones as `github:<owner>/<repo>[@<tag>[/<file>]]`, local ones as
    /// `local/<file>`; enabled in this order, the last one on top.
    #[serde(default)]
    pub packs: Vec<String>,
    /// Game options, written to `options.txt` as `<key>:<value>`.
    #[serde(default)]
    pub options: toml::Table,
}

impl TestProfile {
    /// The file of the profile `name`.
    pub fn path(root: &Path, name: &str) -> PathBuf {
        root.join(DIR).join(format!("{name}.toml"))
    }

    /// The directory of the local files.
    pub fn local_dir(root: &Path) -> PathBuf {
        root.join(DIR).join(LOCAL)
    }

    pub fn load(root: &Path, name: &str) -> Result<Self> {
        check_name(name)?;
        let path = Self::path(root, name);
        if !path.is_file() {
            return Err(Error::usage(
                format!("no test profile `{name}` ({DIR}/{name}.toml is missing)"),
                format!(
                    "create {DIR}/{name}.toml, or pick one of: {}",
                    names(root)?.join(", ")
                ),
            ));
        }
        let profile: Self = parse_toml(
            &read_file(&path)?,
            &format!("{DIR}/{name}.toml"),
            "fix the file: `loader`, `ci`, `mods`, `packs` and an `[options]` table are allowed",
        )?;
        if profile.ci && profile.loader.is_some_and(|l| l != Loader::Fabric) {
            return Err(Error::usage(
                format!("test profile `{name}` runs in CI on a loader other than Fabric"),
                "a CI profile runs Fabric's gametests: set `loader = \"fabric\"`, or remove `ci = true`",
            ));
        }
        for (list, kind) in [
            (&profile.mods, Kind::Mod),
            (&profile.packs, Kind::Resourcepack),
        ] {
            for text in list {
                match Item::parse(text)? {
                    // Versioned with the mod, a profile runs the same mods at
                    // each commit.
                    Item::Platform(wanted) if kind == Kind::Mod && !wanted.is_pinned() => {
                        let slug = &wanted.slug;
                        let hint = if slug.starts_with(github::PREFIX) {
                            format!("pin it as `{slug}@<tag>`, the tag of one of its releases")
                        } else {
                            format!(
                                "pin it as `{slug}@<version>`, a Modrinth version number or id, such as `sodium@mc26.3-0.9.2-fabric`"
                            )
                        };
                        return Err(Error::usage(
                            format!("mod `{slug}` has no version in test profile `{name}`"),
                            hint,
                        ));
                    }
                    Item::Local(file) => check_local(root, name, &file, kind)?,
                    Item::Platform(_) => {}
                }
            }
        }
        for (key, value) in &profile.options {
            option_value(key, value)?;
        }
        Ok(profile)
    }

    /// The game options as `options.txt` writes them. They select the
    /// `custom` graphics preset unless they name one: the game applies any
    /// other preset over the graphics options, such as `renderDistance`.
    pub fn options_lines(&self) -> Result<Vec<(String, String)>> {
        let mut lines = self
            .options
            .iter()
            .map(|(key, value)| Ok((key.clone(), option_value(key, value)?)))
            .collect::<Result<Vec<_>>>()?;
        if !lines.is_empty() && !self.options.contains_key(GRAPHICS_PRESET) {
            lines.push((GRAPHICS_PRESET.to_owned(), "\"custom\"".to_owned()));
        }
        Ok(lines)
    }
}

/// The profiles of the repository, by name, sorted.
pub fn names(root: &Path) -> Result<Vec<String>> {
    let dir = root.join(DIR);
    let mut names = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(names);
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "toml")
            && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            && check_name(name).is_ok()
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// A profile name is also a directory name under `runs/`: lowercase letters,
/// digits, `-` and `_`, starting with a letter or a digit.
pub fn check_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let valid = name.len() <= 64
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(Error::usage(
            format!("`{name}` is not a valid test profile name"),
            "use lowercase letters, digits, `-` and `_`, starting with a letter or a digit",
        ))
    }
}

/// A local file of profile `name` is a file of `kind` in the local directory.
fn check_local(root: &Path, name: &str, file: &str, kind: Kind) -> Result<()> {
    let key = match kind {
        Kind::Mod => "mods",
        Kind::Resourcepack => "packs",
    };
    if Kind::of_file_name(file) != Some(kind) {
        return Err(Error::usage(
            format!(
                "`{LOCAL}/{file}` of test profile `{name}` is not a {}",
                kind.noun()
            ),
            format!("list a `.{}` file under `{key}`", kind.extension()),
        ));
    }
    if !TestProfile::local_dir(root).join(file).is_file() {
        return Err(Error::usage(
            format!("`{LOCAL}/{file}` of test profile `{name}` is missing"),
            format!("put the file in {DIR}/{LOCAL}/ and commit it, or remove it from the profile"),
        ));
    }
    Ok(())
}

/// A mod or a resource pack of a profile: from a platform, or a file of the
/// local directory, written `local/<file>`. A Modrinth slug never holds a
/// `/`, and a GitHub query starts with `github:`: another item that holds a
/// `/` is a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Platform(Wanted),
    /// The file name in the local directory.
    Local(String),
}

impl Item {
    pub fn parse(text: &str) -> Result<Self> {
        if !text.contains('/') || text.trim().starts_with(github::PREFIX) {
            return Wanted::parse(text).map(Self::Platform);
        }
        match text.trim().strip_prefix("local/") {
            Some(file) if !file.is_empty() && !file.contains(['/', '\\']) => {
                Ok(Self::Local(file.to_owned()))
            }
            _ => Err(Error::usage(
                format!(
                    "`{text}` is neither a Modrinth project, a GitHub release nor a file of {DIR}/{LOCAL}/"
                ),
                format!(
                    "write a Modrinth slug, such as `sodium@<version>`, `github:<owner>/<repo>@<tag>`, or `{LOCAL}/<file>` for a file of {DIR}/{LOCAL}/"
                ),
            )),
        }
    }
}

/// A mod or a resource pack asked for: a Modrinth slug, and a version when
/// pinned (its version number or id on Modrinth); or a GitHub query whole,
/// whose tag the platform reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub slug: String,
    pub version: Option<String>,
}

impl Wanted {
    pub fn parse(text: &str) -> Result<Self> {
        if text.trim().starts_with(github::PREFIX) {
            return Ok(Self {
                slug: text.trim().to_owned(),
                version: None,
            });
        }
        let (slug, version) = match text.split_once('@') {
            Some((slug, version)) => (slug.trim(), Some(version.trim())),
            None => (text.trim(), None),
        };
        if slug.is_empty() || slug.contains('/') || version.is_some_and(str::is_empty) {
            return Err(Error::usage(
                format!("`{text}` is not a Modrinth project"),
                "write its Modrinth slug, optionally followed by `@<version>`, such as `sodium` or `sodium@mc26.3-0.9.3-beta.5-fabric`",
            ));
        }
        Ok(Self {
            slug: slug.to_owned(),
            version: version.map(str::to_owned),
        })
    }

    /// Whether it names one version: a Modrinth version, or a GitHub tag.
    pub fn is_pinned(&self) -> bool {
        self.version.is_some() || self.slug.starts_with(github::PREFIX) && self.slug.contains('@')
    }
}

/// A game option's value as `options.txt` holds it: a string as it is, a
/// number or a boolean in its plain form.
fn option_value(key: &str, value: &toml::Value) -> Result<String> {
    let text = match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        _ => {
            return Err(Error::usage(
                format!("option `{key}` of a test profile is not a string, a number or a boolean"),
                "write it as options.txt holds it, such as `renderDistance = 8` or `lang = \"fr_fr\"`",
            ));
        }
    };
    if key.contains(':') || key.contains('\n') || text.contains('\n') {
        return Err(Error::usage(
            format!("option `{key}` of a test profile cannot be written to options.txt"),
            "keep keys without `:` and values on one line",
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, name: &str, text: &str) {
        std::fs::create_dir_all(root.join(DIR)).unwrap();
        std::fs::write(TestProfile::path(root, name), text).unwrap();
    }

    #[test]
    fn a_profile_holds_mods_packs_and_options() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "sodium",
            "loader = \"fabric\"\nmods = [\"sodium@mc26.3-0.9.3-beta.5-fabric\", \"lithium@fWv5bnZk\"]\n\
             packs = [\"faithful-32x\"]\n\n[options]\nrenderDistance = 8\nlang = \"fr_fr\"\nvsync = false\n",
        );
        let profile = TestProfile::load(dir.path(), "sodium").unwrap();
        assert_eq!(profile.loader, Some(Loader::Fabric));
        assert_eq!(profile.mods.len(), 2);
        assert_eq!(profile.packs, ["faithful-32x"]);
        assert_eq!(
            profile.options_lines().unwrap(),
            [
                ("lang".to_owned(), "fr_fr".to_owned()),
                ("renderDistance".to_owned(), "8".to_owned()),
                ("vsync".to_owned(), "false".to_owned()),
                ("graphicsPreset".to_owned(), "\"custom\"".to_owned()),
            ]
        );
    }

    /// The game applies its graphics preset over the graphics options unless
    /// it is `custom`: a profile's options select it, unless the profile sets
    /// a preset itself; a profile without options leaves the game's.
    #[test]
    fn options_select_the_custom_graphics_preset() {
        let fast = TestProfile {
            options: toml::from_str("graphicsPreset = '\"fast\"'\nrenderDistance = 4\n").unwrap(),
            ..TestProfile::default()
        };
        assert_eq!(
            fast.options_lines().unwrap(),
            [
                ("graphicsPreset".to_owned(), "\"fast\"".to_owned()),
                ("renderDistance".to_owned(), "4".to_owned()),
            ]
        );
        assert!(TestProfile::default().options_lines().unwrap().is_empty());
    }

    #[test]
    fn a_ci_profile_runs_on_fabric() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "sodium",
            "ci = true\nmods = [\"sodium@mc26.3-0.9.2-fabric\"]\n",
        );
        assert!(TestProfile::load(dir.path(), "sodium").unwrap().ci);
        write(dir.path(), "fabric", "ci = true\nloader = \"fabric\"\n");
        assert!(TestProfile::load(dir.path(), "fabric").unwrap().ci);

        write(dir.path(), "neoforge", "ci = true\nloader = \"neoforge\"\n");
        let err = TestProfile::load(dir.path(), "neoforge").unwrap_err();
        assert!(err.to_string().contains("other than Fabric"), "{err}");
        assert!(err.hint().contains("loader = \"fabric\""), "{}", err.hint());
    }

    /// A profile is versioned with the mod: the same commit runs the same mods,
    /// in CI too. A resource pack may follow the latest version.
    #[test]
    fn a_profile_pins_the_version_of_its_mods() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "sodium", "ci = true\nmods = [\"sodium\"]\n");
        let err = TestProfile::load(dir.path(), "sodium").unwrap_err();
        assert!(err.to_string().contains("`sodium` has no version"), "{err}");
        assert!(err.hint().contains("sodium@<version>"), "{}", err.hint());

        write(dir.path(), "packs", "packs = [\"faithful-32x\"]\n");
        TestProfile::load(dir.path(), "packs").unwrap();

        // A GitHub mod pins a release tag.
        write(dir.path(), "gh", "mods = [\"github:o/a\"]\n");
        let err = TestProfile::load(dir.path(), "gh").unwrap_err();
        assert!(
            err.to_string().contains("`github:o/a` has no version"),
            "{err}"
        );
        assert!(err.hint().contains("github:o/a@<tag>"), "{}", err.hint());
        write(
            dir.path(),
            "gh",
            "mods = [\"github:o/a@v1/a-fabric.jar\"]\npacks = [\"github:o/p\"]\n",
        );
        let profile = TestProfile::load(dir.path(), "gh").unwrap();
        assert_eq!(
            Item::parse(&profile.mods[0]).unwrap(),
            Item::Platform(Wanted {
                slug: "github:o/a@v1/a-fabric.jar".into(),
                version: None
            })
        );
    }

    /// A file no platform serves is listed from `test-profiles/local/`, in
    /// the same list as the Modrinth ones, so that resource packs keep one
    /// order.
    #[test]
    fn a_profile_names_local_files() {
        let dir = tempfile::tempdir().unwrap();
        let local = TestProfile::local_dir(dir.path());
        std::fs::create_dir_all(&local).unwrap();
        for file in ["touch.jar", "ours.zip"] {
            std::fs::write(local.join(file), b"x").unwrap();
        }
        write(
            dir.path(),
            "local",
            "mods = [\"local/touch.jar\", \"sodium@0.9.2\"]\npacks = [\"faithful\", \"local/ours.zip\"]\n",
        );
        let profile = TestProfile::load(dir.path(), "local").unwrap();
        assert_eq!(
            Item::parse(&profile.packs[1]).unwrap(),
            Item::Local("ours.zip".into())
        );

        for (text, expected, hint) in [
            (
                "mods = [\"local/ours.zip\"]\n",
                "`local/ours.zip` of test profile `bad` is not a mod",
                "`.jar` file under `mods`",
            ),
            (
                "packs = [\"local/touch.jar\"]\n",
                "is not a resource pack",
                "`.zip` file under `packs`",
            ),
            (
                "mods = [\"local/gone.jar\"]\n",
                "`local/gone.jar` of test profile `bad` is missing",
                "test-profiles/local/",
            ),
        ] {
            write(dir.path(), "bad", text);
            let err = TestProfile::load(dir.path(), "bad").unwrap_err();
            assert!(err.to_string().contains(expected), "{text}: {err}");
            assert!(err.hint().contains(hint), "{text}: {}", err.hint());
        }
    }

    #[test]
    fn an_empty_profile_is_valid() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "plain", "");
        assert_eq!(
            TestProfile::load(dir.path(), "plain").unwrap(),
            TestProfile::default()
        );
    }

    #[test]
    fn a_missing_profile_lists_the_others() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "b", "");
        write(dir.path(), "a", "");
        let err = TestProfile::load(dir.path(), "nope").unwrap_err();
        assert!(err.to_string().contains("no test profile `nope`"), "{err}");
        assert_eq!(
            err.hint(),
            "create test-profiles/nope.toml, or pick one of: a, b"
        );
    }

    #[test]
    fn invalid_profiles_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        for (text, expected) in [
            ("mod = [\"x\"]\n", "is invalid"),
            ("loader = \"forge\"\n", "is invalid"),
            ("ci = \"yes\"\n", "is invalid"),
            ("mods = [\"sodium@\"]\n", "is not a Modrinth project"),
            (
                "packs = [\"https://x/y\"]\n",
                "is neither a Modrinth project",
            ),
            (
                "mods = [\"local/a/b.jar\"]\n",
                "is neither a Modrinth project",
            ),
            (
                "mods = [\"other/b.jar\"]\n",
                "is neither a Modrinth project",
            ),
            ("[options]\nkeys = [1]\n", "option `keys`"),
            ("[options]\nlang = \"a\\nb\"\n", "cannot be written"),
        ] {
            write(dir.path(), "bad", text);
            let err = TestProfile::load(dir.path(), "bad").unwrap_err();
            assert!(err.to_string().contains(expected), "{text}: {err}");
        }
    }

    #[test]
    fn names_are_directory_safe() {
        for name in ["sodium", "with-shaders", "a_1", "2"] {
            check_name(name).unwrap();
        }
        for name in [
            "",
            "Sodium",
            "-x",
            ".one-shot",
            "a/b",
            "../x",
            &"a".repeat(65),
        ] {
            assert!(check_name(name).is_err(), "{name}");
        }
    }

    #[test]
    fn wanted_reads_an_optional_version() {
        assert_eq!(
            Wanted::parse("sodium").unwrap(),
            Wanted {
                slug: "sodium".into(),
                version: None
            }
        );
        assert_eq!(
            Wanted::parse("sodium@0.9.3").unwrap().version.as_deref(),
            Some("0.9.3")
        );
        assert!(Wanted::parse("@1").is_err());
    }
}
