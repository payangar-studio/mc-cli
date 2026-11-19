//! Project types and their detection.
//!
//! The one module of the shared crate that names every project type: each
//! binary refuses to run in a repository of another type, so the list of
//! types and their directories has to be known by all of them.

use std::path::{Path, PathBuf};

use crate::error::Error;

/// A type of project, each served by its own binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    Modpack,
    Mod,
}

impl ProjectType {
    pub const ALL: [ProjectType; 2] = [ProjectType::Modpack, ProjectType::Mod];

    /// Directory that marks the root of a repository of this type.
    pub const fn dir(self) -> &'static str {
        match self {
            ProjectType::Modpack => ".modpack",
            ProjectType::Mod => ".mod",
        }
    }

    /// Binary that works in a repository of this type.
    pub const fn binary(self) -> &'static str {
        match self {
            ProjectType::Modpack => "modpack",
            ProjectType::Mod => "mod",
        }
    }
}

/// A repository of a known type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub kind: ProjectType,
    pub root: PathBuf,
}

impl Project {
    /// Walks up from `start` to the first directory that marks the root of
    /// a repository of any type.
    pub fn find(start: &Path) -> Option<Self> {
        start.ancestors().find_map(|dir| {
            ProjectType::ALL
                .into_iter()
                .find(|kind| dir.join(kind.dir()).is_dir())
                .map(|kind| Self {
                    kind,
                    root: dir.to_path_buf(),
                })
        })
    }

    /// Refusal of `binary`, which serves another type, to run in this
    /// repository.
    pub fn wrong_type(&self, binary: &str) -> Error {
        let own = self.kind.binary();
        Error::usage(
            format!(
                "{} is a {own} repository ({}/ found), not one `{binary}` works in",
                self.root.display(),
                self.kind.dir(),
            ),
            format!("use the `{own}` binary here"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn repo(kind: ProjectType) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(kind.dir())).unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        dir
    }

    #[test]
    fn each_type_is_found_from_a_nested_directory() {
        for kind in ProjectType::ALL {
            let dir = repo(kind);
            let expected = Project {
                kind,
                root: dir.path().to_path_buf(),
            };
            assert_eq!(Project::find(&dir.path().join("a/b")), Some(expected));
        }
    }

    #[test]
    fn the_nearest_repository_wins() {
        let outer = repo(ProjectType::Modpack);
        let inner = outer.path().join("a");
        std::fs::create_dir(inner.join(".mod")).unwrap();
        let found = Project::find(&inner.join("b")).unwrap();
        assert_eq!(found.kind, ProjectType::Mod);
        assert_eq!(found.root, inner);
    }

    #[test]
    fn nothing_is_found_outside_a_repository() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Project::find(dir.path()), None);
    }

    #[test]
    fn a_file_named_like_the_marker_is_not_a_repository() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".mod"), "").unwrap();
        assert_eq!(Project::find(dir.path()), None);
    }

    #[test]
    fn the_refusal_names_the_binary_to_use() {
        let dir = repo(ProjectType::Mod);
        let err = Project::find(dir.path()).unwrap().wrong_type("modpack");
        assert_eq!(err.kind(), ErrorKind::Usage);
        assert!(err.hint().contains("`mod`"), "{}", err.hint());
    }
}
