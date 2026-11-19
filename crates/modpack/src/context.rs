//! Context shared by every command handler.

use std::path::Path;

use mc_core::project::{Project, ProjectType};

use crate::error::{Error, Result};
use crate::prompt::Prompt;

/// Name of the directory that marks the root of a modpack repository.
pub const SOCLE_DIR: &str = ProjectType::Modpack.dir();

#[derive(Debug, Clone)]
pub struct Context {
    /// Repository the working directory is inside, of any type.
    pub project: Option<Project>,
    pub prompt: Prompt,
    /// `-v` count minus `-q` count.
    pub verbosity: i8,
}

impl Context {
    /// Builds the context from the current directory and the process environment.
    pub fn detect(verbosity: i8) -> Result<Self> {
        let cwd = std::env::current_dir()?;
        let ctx = Self {
            project: Project::find(&cwd),
            prompt: Prompt::new(crate::prompt::detect_interactive()),
            verbosity,
        };
        tracing::debug!(project = ?ctx.project, interactive = ctx.prompt.is_interactive(), "context");
        Ok(ctx)
    }

    /// The repository root, for commands that only work inside a modpack
    /// repository. Checks the repository's socle version first: a newer socle
    /// is refused, an older one gives a warning.
    pub fn require_root(&self) -> Result<&Path> {
        let root = self.require_root_unchecked()?;
        crate::socle::check_version(root)?;
        Ok(root)
    }

    /// The repository root, without the socle version check. Only for
    /// `modpack update`, which handles the version itself. A repository of
    /// another type is refused.
    pub fn require_root_unchecked(&self) -> Result<&Path> {
        match &self.project {
            Some(project) if project.kind == ProjectType::Modpack => Ok(&project.root),
            Some(project) => Err(project.wrong_type(ProjectType::Modpack.binary())),
            None => Err(Error::usage(
                format!(
                    "not inside a modpack repository (no {SOCLE_DIR}/ found in this directory or above)"
                ),
                "run the command from a modpack repository, or create one with `modpack init`",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use std::path::PathBuf;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    fn context(cwd: &Path) -> Context {
        Context {
            project: Project::find(cwd),
            prompt: Prompt::new(false),
            verbosity: 0,
        }
    }

    #[test]
    fn root_is_found_from_a_nested_directory() {
        let repo = fixtures().join("repo");
        let ctx = context(&repo.join("config/nested"));
        assert_eq!(ctx.require_root_unchecked().unwrap(), repo);
        assert_eq!(context(&repo).require_root_unchecked().unwrap(), repo);
    }

    #[test]
    fn no_root_outside_a_repository() {
        let ctx = context(&fixtures());
        assert_eq!(ctx.project, None);
        let err = ctx.require_root().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Usage);
        assert!(err.hint().contains("modpack init"));
    }

    #[test]
    fn a_mod_repository_is_refused() {
        let ctx = context(&fixtures().join("mod/src"));
        let err = ctx.require_root().unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Usage);
        assert!(err.hint().contains("`mod`"), "{}", err.hint());
    }
}
