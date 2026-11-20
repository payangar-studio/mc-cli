//! The mod socle: what `mod init` lays down and `mod update` rewrites, through
//! the shared socle mechanics ([`mc_core::socle`]).
//!
//! - `templates/socle/`: files the socle owns, written at the same relative
//!   path, recorded in the registry and rewritten by every update: the Gradle
//!   wrapper, the root settings, `build-logic/`, which holds the whole build,
//!   fills the loader metadata from `mod.toml` and configures the publication,
//!   the publish workflow, the git hooks (`.mod/hooks/`) and the harness: the
//!   `editing-agents-md` skill, its check under `.mod/scripts/` and its
//!   workflow;
//! - `templates/blocks/`: blocks between markers in a file the mod owns
//!   otherwise (`AGENTS.md`, `.gitignore`);
//! - `templates/skeleton/`: the mod's own files, written once by `init` when
//!   missing ([`skeleton`]): `mod.toml`, the subproject build scripts, the
//!   entrypoints, the page. Their paths depend on the package, the loaders and
//!   the side, so they are laid out by code rather than by their place in the
//!   templates.
//!
//! Adding a file to `templates/socle/` needs no code change.

pub mod skeleton;

use std::path::Path;

use include_dir::{Dir, include_dir};
use mc_core::socle::Socle;
use mc_core::socle::blocks::Markers;

use crate::context::SOCLE_DIR;
use crate::error::{Context as _, Result};
use crate::model::mod_toml::ModToml;
use crate::model::socle::Versions;

static FILES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/socle");
static BLOCKS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/blocks");
/// The skeleton is laid out by [`skeleton`], not by the shared mechanics.
static PROJECT: Dir<'_> = Dir::new("", &[]);

/// Markers of a Markdown file.
pub const HTML: Markers = Markers {
    begin: "<!-- mod:begin -->",
    end: "<!-- mod:end -->",
};

/// Markers of a file whose comments start with `#`.
pub const HASH: Markers = Markers {
    begin: "# mod:begin",
    end: "# mod:end",
};

/// The mod socle.
pub static SOCLE: Socle = Socle {
    dir: SOCLE_DIR,
    bin: "mod",
    tool: "mc-cli",
    version: env!("CARGO_PKG_VERSION"),
    files: &FILES,
    blocks: &BLOCKS,
    project: &PROJECT,
    block_targets: &[
        ("agents.md", "AGENTS.md", HTML),
        ("gitignore", ".gitignore", HASH),
    ],
    html: HTML,
    hash: HASH,
};

/// Checks the repository's socle version against the CLI's, at the start of
/// every command that works in a repository, and enables the socle's git hooks
/// in a clone, silently: `init` and `update` say why they do not run.
pub fn check_version(root: &Path) -> Result<()> {
    SOCLE.check_version::<Versions>(root)?;
    enable_hooks(root);
    Ok(())
}

/// Socle files that already exist in `root` with other contents: those `init`
/// would overwrite.
pub fn overwritten(root: &Path) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for template in SOCLE.socle_files() {
        let path = template
            .path
            .split('/')
            .fold(root.to_path_buf(), |p, c| p.join(c));
        if path.symlink_metadata().is_err() {
            continue;
        }
        let same = path.is_file()
            && std::fs::read(&path).context(format!("reading {}", path.display()))?
                == template.contents;
        if !same {
            paths.push(template.path);
        }
    }
    Ok(paths)
}

/// Creates a mod repository in `root`, which must not hold `.mod/` yet: the
/// mod's skeleton (`mod.toml` first), then the socle files and blocks, the
/// registry, and the socle settings with the pinned `versions`.
///
/// Files of the mod that already exist are kept as they are.
pub fn init(root: &Path, module: &ModToml, versions: Versions) -> Result<()> {
    for file in skeleton::files(module) {
        let path = file
            .path
            .split('/')
            .fold(root.to_path_buf(), |p, c| p.join(c));
        if path.exists() {
            continue;
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).context(format!("creating {}", dir.display()))?;
        }
        std::fs::write(&path, file.contents).context(format!("writing {}", path.display()))?;
    }
    SOCLE.init(root, versions)?;
    if let Some(warning) = enable_hooks(root) {
        tracing::warn!("{warning}");
    }
    Ok(())
}

/// Value of `core.hooksPath`: the socle's git hooks (`.mod/hooks/`).
pub const HOOKS_PATH: &str = ".mod/hooks";

/// Points `core.hooksPath` of the repository at the socle's hooks when git has
/// no value for it at any level: `core.hooksPath` is local configuration, which
/// a clone does not get. A value set elsewhere (a hooks manager) is kept.
/// Returns why the socle's hooks do not run, if they do not; never fails: the
/// CI checks again what the hooks check.
pub fn enable_hooks(root: &Path) -> Option<String> {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    match git(&["rev-parse", "--show-prefix"]).as_deref() {
        Some("") => {}
        Some(prefix) => {
            return Some(format!(
                "the mod is in {prefix} of its git repository, not at its root: git looks for {HOOKS_PATH} from the root, so the socle's hooks do not run"
            ));
        }
        None => return Some("not a git repository: the socle's git hooks do not run".into()),
    }
    match git(&["config", "--default", "", "--get", "core.hooksPath"]).as_deref() {
        Some("") => git(&["config", "--local", "core.hooksPath", HOOKS_PATH])
            .is_none()
            .then(|| {
                format!(
                    "could not enable the socle's git hooks; run `git config core.hooksPath {HOOKS_PATH}`"
                )
            }),
        Some(HOOKS_PATH) => None,
        Some(other) => Some(format!(
            "core.hooksPath is `{other}`: the socle's hooks in {HOOKS_PATH} do not run, run them from your own hooks"
        )),
        None => Some(format!(
            "could not read core.hooksPath; run `git config core.hooksPath {HOOKS_PATH}` to enable the socle's git hooks"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_block_template_has_a_target() {
        for file in BLOCKS.files() {
            let name = file.path().to_str().unwrap();
            assert!(
                SOCLE.block_targets.iter().any(|(t, _, _)| *t == name),
                "templates/blocks/{name} has no entry in SOCLE.block_targets"
            );
        }
        assert_eq!(SOCLE.socle_blocks().len(), SOCLE.block_targets.len());
    }

    #[test]
    fn the_build_is_embedded() {
        let paths: Vec<_> = SOCLE.socle_files().into_iter().map(|t| t.path).collect();
        for path in [
            "settings.gradle.kts",
            "gradlew",
            "gradle/wrapper/gradle-wrapper.jar",
            "build-logic/build.gradle.kts",
            "build-logic/templates/fabric.mod.json",
            "build-logic/templates/neoforge.mods.toml",
        ] {
            assert!(paths.contains(&path.to_owned()), "{path} missing");
        }
    }

    #[test]
    fn the_wrapper_is_gradle_9() {
        let file = FILES
            .get_file("gradle/wrapper/gradle-wrapper.properties")
            .unwrap();
        let text = file.contents_utf8().unwrap();
        assert!(text.contains("/gradle-9."), "{text}");
        assert!(text.contains("distributionSha256Sum="), "{text}");
    }
}
