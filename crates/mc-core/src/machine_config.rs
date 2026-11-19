//! The machine configuration, shared by every binary:
//! `<user config dir>/minecraft/config.toml`.
//!
//! It holds the author identity (`[identity]`) and, later, preferences. Only
//! the `init` of a project reads the identity, as defaults of its arguments,
//! and copies the values into the project: CI and other machines never depend
//! on this file.
//!
//! ```toml
//! [identity]
//! author = "Payangar"
//! package = "com.payangar"
//! ```

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::model::{parse_toml, read_file, write_file};
use crate::prompt::Prompt;

/// Table of the identity in the file.
const IDENTITY: &str = "identity";

/// Where the file lives on this machine.
pub fn default_path() -> Result<PathBuf> {
    let dirs = directories::BaseDirs::new().ok_or_else(|| {
        Error::internal(
            "cannot find the user's home directory",
            "set HOME to the user's home directory",
        )
    })?;
    Ok(dirs.config_dir().join("minecraft").join("config.toml"))
}

/// A value of the identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityField {
    /// Author name, such as `Payangar`.
    Author,
    /// Base Java package, such as `com.payangar`; a mod uses `<package>.<modid>`.
    Package,
}

impl IdentityField {
    /// Key under `[identity]`.
    pub fn key(self) -> &'static str {
        match self {
            IdentityField::Author => "author",
            IdentityField::Package => "package",
        }
    }

    fn question(self) -> &'static str {
        match self {
            IdentityField::Author => "Author",
            IdentityField::Package => "Base Java package",
        }
    }
}

/// The identity as the file holds it. Unknown keys and tables are ignored:
/// the binaries share the file and may not be at the same version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Identity {
    pub author: Option<String>,
    pub package: Option<String>,
}

impl Identity {
    /// Reads the identity; a missing file is a machine with no identity yet.
    pub fn load(path: &Path) -> Result<Self> {
        #[derive(Deserialize)]
        struct File {
            #[serde(default)]
            identity: Identity,
        }
        if !path.exists() {
            return Ok(Self::default());
        }
        let file: File = parse_toml(
            &read_file(path)?,
            &path.display().to_string(),
            "fix the file, or delete it and pass the values as arguments",
        )?;
        Ok(file.identity)
    }

    /// The value of `field`; an empty one counts as missing.
    pub fn get(&self, field: IdentityField) -> Option<&str> {
        let value = match field {
            IdentityField::Author => &self.author,
            IdentityField::Package => &self.package,
        };
        value.as_deref().filter(|v| !v.trim().is_empty())
    }
}

/// Writes `value` as `field` of the identity in `path`, keeping the rest of
/// the file (other values, other tables, comments) as it is.
pub fn save(path: &Path, field: IdentityField, value: &str) -> Result<()> {
    let text = if path.exists() {
        read_file(path)?
    } else {
        String::new()
    };
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| {
        Error::usage(
            format!("{} is invalid: {}", path.display(), e.message()),
            "fix the file, or delete it and pass the values as arguments",
        )
    })?;
    let table = doc
        .entry(IDENTITY)
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(|| {
            Error::usage(
                format!("`{IDENTITY}` in {} is not a table", path.display()),
                format!("make it a `[{IDENTITY}]` table, or delete the file"),
            )
        })?;
    table[field.key()] = toml_edit::value(value);
    write_file(path, &doc.to_string())
}

/// The identity values an `init` takes as defaults of its arguments. The file
/// is read only when an argument is missing, at most once.
pub struct MachineIdentity {
    path: Option<PathBuf>,
    identity: Option<Identity>,
}

impl Default for MachineIdentity {
    fn default() -> Self {
        Self::new()
    }
}

impl MachineIdentity {
    /// The file at its usual place on this machine.
    pub fn new() -> Self {
        Self {
            path: None,
            identity: None,
        }
    }

    /// The file at `path`.
    pub fn at(path: PathBuf) -> Self {
        Self {
            path: Some(path),
            identity: None,
        }
    }

    /// The value of an `init` argument: the argument when given, else the
    /// machine file. Missing from both, it is asked for in a terminal, with an
    /// offer to save it in the file; without a terminal it is an error naming
    /// `arg`.
    pub fn resolve(
        &mut self,
        prompt: &Prompt,
        value: Option<String>,
        field: IdentityField,
        arg: &str,
    ) -> Result<String> {
        if let Some(value) = value {
            return Ok(value);
        }
        let path = self.path()?;
        if let Some(value) = self.identity()?.get(field) {
            return Ok(value.to_owned());
        }
        if !prompt.is_interactive() {
            return Err(Error::usage(
                format!("missing value for {arg}, and no terminal to ask for it"),
                format!(
                    "pass {arg}, or set `{}` under `[{IDENTITY}]` in {}",
                    field.key(),
                    path.display()
                ),
            ));
        }
        let value = prompt.text(None, arg, field.question())?;
        let question = format!(
            "Save it in {} as the default of the next projects?",
            path.display()
        );
        if prompt.confirm(None, arg, &question, true)? {
            save(&path, field, &value)?;
            self.identity = None;
        }
        Ok(value)
    }

    fn path(&mut self) -> Result<PathBuf> {
        if self.path.is_none() {
            self.path = Some(default_path()?);
        }
        Ok(self.path.clone().expect("set above"))
    }

    fn identity(&mut self) -> Result<&Identity> {
        if self.identity.is_none() {
            self.identity = Some(Identity::load(&self.path()?)?);
        }
        Ok(self.identity.as_ref().expect("set above"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn file(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("minecraft/config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        (dir, path)
    }

    #[test]
    fn file_is_in_the_user_config_dir() {
        let path = default_path().unwrap();
        assert!(path.ends_with("minecraft/config.toml"));
        let config = directories::BaseDirs::new()
            .unwrap()
            .config_dir()
            .to_owned();
        assert!(path.starts_with(config));
    }

    #[test]
    fn value_comes_from_the_file_without_asking() {
        let (_dir, path) = file("[identity]\nauthor = \"Payangar\"\npackage = \"com.payangar\"\n");
        // Interactive on purpose: asking would fail or hang without a terminal.
        let prompt = Prompt::new(true);
        let mut machine = MachineIdentity::at(path);
        let author = machine.resolve(&prompt, None, IdentityField::Author, "--author");
        assert_eq!(author.unwrap(), "Payangar");
        let package = machine.resolve(&prompt, None, IdentityField::Package, "--package");
        assert_eq!(package.unwrap(), "com.payangar");
    }

    #[test]
    fn argument_wins_and_the_file_is_not_read() {
        let (_dir, path) = file("not toml at all [");
        let mut machine = MachineIdentity::at(path);
        let value = machine.resolve(
            &Prompt::new(false),
            Some("Arg".into()),
            IdentityField::Author,
            "--author",
        );
        assert_eq!(value.unwrap(), "Arg");
    }

    #[test]
    fn missing_without_terminal_names_the_argument_and_the_file() {
        for text in [
            "",
            "[identity]\nauthor = \"\"\npackage = \"com.payangar\"\n",
        ] {
            let (_dir, path) = file(text);
            let err = MachineIdentity::at(path.clone())
                .resolve(&Prompt::new(false), None, IdentityField::Author, "--author")
                .unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Usage);
            assert!(err.hint().starts_with("pass --author"), "{}", err.hint());
            assert!(err.hint().contains(&path.display().to_string()));
        }
    }

    #[test]
    fn missing_file_is_no_identity() {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity::load(&dir.path().join("config.toml")).unwrap();
        assert_eq!(identity, Identity::default());
    }

    #[test]
    fn unknown_keys_and_tables_are_ignored() {
        let (_dir, path) =
            file("[identity]\nauthor = \"A\"\nemail = \"x\"\n\n[preferences]\nx = 1\n");
        assert_eq!(
            Identity::load(&path).unwrap().get(IdentityField::Author),
            Some("A")
        );
    }

    #[test]
    fn invalid_file_is_a_usage_error_naming_it() {
        let (_dir, path) = file("[identity]\nauthor = 3\n");
        let err = Identity::load(&path).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Usage);
        assert!(err.to_string().contains(&path.display().to_string()));
    }

    #[test]
    fn save_creates_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("minecraft/config.toml");
        save(&path, IdentityField::Author, "Payangar").unwrap();
        assert_eq!(
            Identity::load(&path).unwrap().get(IdentityField::Author),
            Some("Payangar")
        );
    }

    #[test]
    fn save_keeps_the_rest_of_the_file() {
        let (_dir, path) =
            file("# mine\n[identity]\npackage = \"com.payangar\"\n\n[preferences]\nx = 1\n");
        save(&path, IdentityField::Author, "Payangar").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# mine\n"), "{text}");
        assert!(text.contains("[preferences]\nx = 1\n"), "{text}");
        let identity = Identity::load(&path).unwrap();
        assert_eq!(identity.get(IdentityField::Author), Some("Payangar"));
        assert_eq!(identity.get(IdentityField::Package), Some("com.payangar"));
    }
}
