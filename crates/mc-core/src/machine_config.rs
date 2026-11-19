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

use crate::error::{Context, Error, Result};
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
        value.as_deref().map(str::trim).filter(|v| !v.is_empty())
    }
}

/// Writes `value` as `field` of the identity in `path`, keeping the rest of
/// the file (other values, other tables, comments) as it is. A symlink is
/// written through to its target and the file keeps its permissions: the file
/// is often a dotfile managed elsewhere.
pub fn save(path: &Path, field: IdentityField, value: &str) -> Result<()> {
    let (path, permissions) = if path.exists() {
        let target =
            std::fs::canonicalize(path).context(format!("resolving {}", path.display()))?;
        let permissions = std::fs::metadata(&target)
            .context(format!("reading {}", target.display()))?
            .permissions();
        (target, Some(permissions))
    } else {
        (path.to_owned(), None)
    };
    let path = path.as_path();
    let text = if permissions.is_some() {
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
        .as_table_like_mut()
        .ok_or_else(|| {
            Error::usage(
                format!("`{IDENTITY}` in {} is not a table", path.display()),
                format!("make it a `[{IDENTITY}]` table, or delete the file"),
            )
        })?;
    table.insert(field.key(), toml_edit::value(value));
    write_file(path, &doc.to_string())?;
    if let Some(permissions) = permissions {
        std::fs::set_permissions(path, permissions)
            .context(format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// The identity values an `init` takes as defaults of its arguments. The file
/// is read only when an argument is missing, at most once.
pub struct MachineIdentity {
    /// `None` when this machine has no config directory: no file to read or
    /// save, the values come from the arguments or the terminal.
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
        let path = default_path()
            .inspect_err(|e| tracing::debug!("no machine configuration: {e}"))
            .ok();
        Self {
            path,
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
        self.resolve_with(prompt.is_interactive(), value, field, arg, |path| {
            let value = prompt.text(None, arg, field.question())?;
            let Some(path) = path else {
                return Ok((value, false));
            };
            let question = format!(
                "Save it in {} as the default of the next projects?",
                path.display()
            );
            let save = prompt.confirm(None, arg, &question, true)?;
            Ok((value, save))
        })
    }

    /// `resolve`, with `ask` standing for the terminal: given the file, if
    /// any, it returns the answer and whether to save it there.
    fn resolve_with(
        &mut self,
        interactive: bool,
        value: Option<String>,
        field: IdentityField,
        arg: &str,
        ask: impl FnOnce(Option<&Path>) -> Result<(String, bool)>,
    ) -> Result<String> {
        if let Some(value) = value {
            return Ok(value);
        }
        if let Some(value) = self.identity()?.get(field) {
            return Ok(value.to_owned());
        }
        if !interactive {
            return Err(match &self.path {
                Some(path) => Error::usage(
                    format!("missing value for {arg}, and no terminal to ask for it"),
                    format!(
                        "pass {arg}, or set `{}` under `[{IDENTITY}]` in {}",
                        field.key(),
                        path.display()
                    ),
                ),
                None => Error::missing_value(arg),
            });
        }
        let (value, keep) = ask(self.path.as_deref())?;
        if let (true, Some(path)) = (keep, &self.path) {
            // Saving is a convenience: failing it must not lose the answer.
            match save(path, field, &value) {
                Ok(()) => self.identity = None,
                Err(e) => tracing::warn!("could not save {arg} in {}: {e}", path.display()),
            }
        }
        Ok(value)
    }

    fn identity(&mut self) -> Result<&Identity> {
        if self.identity.is_none() {
            let identity = match &self.path {
                Some(path) => Identity::load(path)?,
                None => Identity::default(),
            };
            self.identity = Some(identity);
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

    #[test]
    fn save_into_an_inline_identity_table() {
        let (_dir, path) = file("identity = { package = \"com.payangar\" }\n");
        save(&path, IdentityField::Author, "Payangar").unwrap();
        let identity = Identity::load(&path).unwrap();
        assert_eq!(identity.get(IdentityField::Author), Some("Payangar"));
        assert_eq!(identity.get(IdentityField::Package), Some("com.payangar"));
    }

    #[cfg(unix)]
    #[test]
    fn save_writes_through_a_symlink_and_keeps_the_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (_dotfiles, target) = file("[identity]\npackage = \"com.payangar\"\n");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("config.toml");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        save(&link, IdentityField::Author, "Payangar").unwrap();

        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(
            Identity::load(&target).unwrap().get(IdentityField::Author),
            Some("Payangar")
        );
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn answer_survives_a_failed_save() {
        let dir = tempfile::tempdir().unwrap();
        // A regular file where the directory should be: the save cannot write.
        std::fs::write(dir.path().join("minecraft"), "").unwrap();
        let mut machine = MachineIdentity::at(dir.path().join("minecraft/config.toml"));
        let value = machine.resolve_with(true, None, IdentityField::Author, "--author", |_| {
            Ok(("Asked".into(), true))
        });
        assert_eq!(value.unwrap(), "Asked");
    }

    #[test]
    fn no_config_dir_asks_or_names_the_argument() {
        let mut machine = MachineIdentity {
            path: None,
            identity: None,
        };
        let err = machine
            .resolve_with(false, None, IdentityField::Author, "--author", |_| {
                panic!("asked without a terminal")
            })
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Usage);
        assert_eq!(err.hint(), "pass --author");

        let value = machine.resolve_with(true, None, IdentityField::Author, "--author", |path| {
            assert_eq!(path, None, "no file to offer saving into");
            Ok(("Asked".into(), true))
        });
        assert_eq!(value.unwrap(), "Asked");
    }

    #[test]
    fn values_are_trimmed() {
        let (_dir, path) = file("[identity]\nauthor = \" Payangar \"\n");
        assert_eq!(
            Identity::load(&path).unwrap().get(IdentityField::Author),
            Some("Payangar")
        );
    }
}
