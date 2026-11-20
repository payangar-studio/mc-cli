//! `test-profiles/<name>.toml`: a named set of extra mods, resource packs and
//! game options to run the mod with, versioned with the mod. `mod run
//! --profile <name>` runs it in `runs/<name>/`, ignored by git. A profile with
//! `ci = true` also runs in the mod's CI, as Fabric gametests with its mods
//! (`.github/workflows/mod-tests.yml`).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::model::mod_toml::Loader;
use mc_core::model::{parse_toml, read_file};

/// Directory of the profiles, at the repository root.
pub const DIR: &str = "test-profiles";
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
    /// Modrinth mods, as `<slug>@<version>`: pinned.
    #[serde(default)]
    pub mods: Vec<String>,
    /// Modrinth resource packs, as `<slug>` or `<slug>@<version>`, enabled in
    /// this order, the last one on top.
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
        for wanted in profile.mods.iter().chain(&profile.packs) {
            Wanted::parse(wanted)?;
        }
        // Versioned with the mod, a profile runs the same mods at each commit.
        for wanted in &profile.mods {
            let Wanted { slug, version } = Wanted::parse(wanted)?;
            if version.is_none() {
                return Err(Error::usage(
                    format!("mod `{slug}` has no version in test profile `{name}`"),
                    format!(
                        "pin it as `{slug}@<version>`, a Modrinth version number or id, such as `sodium@mc26.3-0.9.2-fabric`"
                    ),
                ));
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

/// A mod or a resource pack asked for: a Modrinth slug, and a version when
/// pinned (its version number or id on Modrinth).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub slug: String,
    pub version: Option<String>,
}

impl Wanted {
    pub fn parse(text: &str) -> Result<Self> {
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
            ("packs = [\"https://x/y\"]\n", "is not a Modrinth project"),
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
