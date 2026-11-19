//! Context shared by every command handler.

use std::path::Path;

use mc_core::project::{Project, ProjectType};

use crate::error::{Error, Result};
use crate::prompt::Prompt;

/// Name of the directory that marks the root of a mod repository.
pub const SOCLE_DIR: &str = ProjectType::Mod.dir();

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

    /// The repository root, for commands that only work inside a mod
    /// repository. Checks the repository's socle version first: a newer socle
    /// is refused, an older one gives a warning.
    pub fn require_root(&self) -> Result<&Path> {
        let root = self.require_root_unchecked()?;
        crate::socle::check_version(root)?;
        Ok(root)
    }

    /// The repository root, without the socle version check. Only for
    /// `mod update`, which handles the version itself. A repository of
    /// another type is refused.
    pub fn require_root_unchecked(&self) -> Result<&Path> {
        match &self.project {
            Some(project) if project.kind == ProjectType::Mod => Ok(&project.root),
            Some(project) => Err(project.wrong_type(ProjectType::Mod.binary())),
            None => Err(Error::usage(
                format!(
                    "not inside a mod repository (no {} found in this directory or above)",
                    ProjectType::Mod.marker()
                ),
                "run the command from a mod repository, or create one with `mod init`",
            )),
        }
    }
}
