//! The modpack socle: what `modpack init` lays down and `modpack update`
//! rewrites, through the shared socle mechanics ([`mc_core::socle`]).
//!
//! The templates are embedded in the binary, in three families:
//! - `templates/socle/`: files the socle owns, written at the same relative
//!   path, recorded in the registry and rewritten by every update;
//! - `templates/blocks/`: blocks between markers in a file the pack owns
//!   otherwise, recorded in the registry as blocks;
//! - `templates/project/`: the skeleton of the pack's content, written once by
//!   `init` when missing, never recorded, never touched again.
//!
//! Adding a template file to `templates/socle/` or `templates/project/` needs
//! no code change.

use std::path::Path;

use include_dir::{Dir, include_dir};
use mc_core::socle::Socle;
use mc_core::socle::blocks::Markers;

use crate::context::SOCLE_DIR;
use crate::error::Result;
use crate::model::manifest::{self, Manifest, Pack};
use crate::model::socle::InitValues;

static FILES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/socle");
static BLOCKS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/blocks");
static PROJECT: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/project");

/// Markers of a Markdown file.
pub const HTML: Markers = Markers {
    begin: "<!-- modpack:begin -->",
    end: "<!-- modpack:end -->",
};

/// Markers of a file whose comments start with `#`.
pub const HASH: Markers = Markers {
    begin: "# modpack:begin",
    end: "# modpack:end",
};

/// The modpack socle.
pub static SOCLE: Socle = Socle {
    dir: SOCLE_DIR,
    bin: "modpack",
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
/// every command that works in a repository: a newer socle is refused, an
/// older one gives a warning, once per process, that suggests `modpack update`.
pub fn check_version(root: &Path) -> Result<()> {
    SOCLE.check_version::<InitValues>(root)
}

/// Creates a modpack repository in `root`, which must not hold `.modpack/`
/// yet: socle settings, empty manifest, pack skeleton, socle files and
/// blocks, registry, and `CLAUDE.md` pointing at `AGENTS.md`.
///
/// Pack files that already exist (the manifest, the skeleton) are kept as
/// they are: the socle never overwrites the pack's content.
pub fn init(root: &Path, values: InitValues) -> Result<()> {
    let manifest_path = root.join(manifest::FILE_NAME);
    if !manifest_path.exists() {
        Manifest::new(Pack {
            minecraft: values.minecraft.clone(),
            loader: values.loader.clone(),
            loader_version: values.loader_version.clone(),
        })
        .save(&manifest_path)?;
    }
    SOCLE.init(root, values)
}
#[cfg(test)]
mod tests {
    use std::fs;

    use mc_core::socle::blocks;

    use super::*;
    use crate::model::socle::{LockedFile, LockedKind, SocleConfig, SocleLock};
    use crate::model::variants::Variants;
    use crate::model::{sha256_bytes, sha256_file};

    const AGENTS_MD: &str = "AGENTS.md";
    const CLAUDE_MD: &str = "CLAUDE.md";

    fn values() -> InitValues {
        InitValues {
            name: "Test Pack".into(),
            minecraft: "1.21.1".into(),
            loader: "neoforge".into(),
            loader_version: "21.1.77".into(),
            author: "Tester".into(),
        }
    }

    fn initialised() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), values()).unwrap();
        dir
    }

    #[test]
    fn hidden_template_files_are_embedded() {
        let paths: Vec<_> = SOCLE.socle_files().into_iter().map(|t| t.path).collect();
        assert!(paths.contains(&".modpack/README.md".to_owned()));
        let paths: Vec<_> = SOCLE.project_files().into_iter().map(|t| t.path).collect();
        for path in [
            "config/.gitkeep",
            "mods/.gitkeep",
            "variants.toml",
            "overrides/.gitkeep",
        ] {
            assert!(paths.contains(&path.to_owned()), "{path} missing");
        }
    }

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
    fn skeleton_variants_have_a_default_variant_and_no_condition() {
        let template = SOCLE
            .project_files()
            .into_iter()
            .find(|t| t.path == "variants.toml")
            .unwrap();
        let variants = Variants::parse(std::str::from_utf8(template.contents).unwrap()).unwrap();
        assert!(variants.conditions.is_empty());
        assert!(variants.default.conditions.is_empty());
        assert!(variants.default.remove.is_empty());
        assert!(variants.variants.is_empty());
    }

    #[test]
    fn init_records_every_file_it_lays_down() {
        let dir = initialised();
        let root = dir.path();
        let lock = SocleLock::load(&SOCLE, root).unwrap();
        assert_eq!(
            lock.files.len(),
            SOCLE.socle_files().len() + SOCLE.socle_blocks().len()
        );
        assert!(SOCLE.edited_by_hand(root, &lock).unwrap().is_empty());
        for entry in &lock.files {
            if entry.kind == LockedKind::File {
                assert_eq!(sha256_file(&root.join(&entry.path)).unwrap(), entry.sha256);
            }
        }
        let config = SocleConfig::load(&SOCLE, root).unwrap();
        assert_eq!(config.version, SOCLE.cli_version().to_string());
        assert_eq!(config.init, values());
        let manifest = Manifest::load(&root.join(manifest::FILE_NAME)).unwrap();
        assert_eq!(manifest.pack.loader_version, "21.1.77");
        assert!(manifest.entries.is_empty());
        assert!(root.join("variants.toml").is_file());
        assert!(root.join("overrides").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn init_links_claude_md_to_agents_md() {
        let dir = initialised();
        let link = dir.path().join(CLAUDE_MD);
        assert_eq!(fs::read_link(&link).unwrap(), Path::new(AGENTS_MD));
    }

    #[test]
    fn init_keeps_existing_pack_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("variants.toml"), "# mine\n").unwrap();
        fs::write(dir.path().join(CLAUDE_MD), "mine\n").unwrap();
        fs::write(dir.path().join(".gitignore"), "target/\n").unwrap();
        init(dir.path(), values()).unwrap();
        let read = |p: &str| fs::read_to_string(dir.path().join(p)).unwrap();
        assert_eq!(read("variants.toml"), "# mine\n");
        assert_eq!(read(CLAUDE_MD), "mine\n");
        assert!(read(".gitignore").starts_with("target/\n\n# modpack:begin\n"));
    }

    #[test]
    fn hand_edits_are_detected() {
        let dir = initialised();
        let root = dir.path();
        let lock = SocleLock::load(&SOCLE, root).unwrap();

        // Text around a block belongs to the pack.
        let agents = root.join(AGENTS_MD);
        let text = fs::read_to_string(&agents).unwrap();
        fs::write(&agents, format!("# Mine\n\n{text}\nMore.\n")).unwrap();
        assert!(SOCLE.edited_by_hand(root, &lock).unwrap().is_empty());

        fs::write(root.join(".modpack/README.md"), "edited\n").unwrap();
        fs::write(&agents, blocks::upsert(&text, HTML, "edited block\n")).unwrap();
        fs::remove_file(root.join(".gitignore")).unwrap();
        assert_eq!(
            SOCLE.edited_by_hand(root, &lock).unwrap(),
            [".gitignore", ".modpack/README.md", AGENTS_MD]
        );
    }

    #[test]
    fn stale_files_and_blocks_are_removed() {
        let dir = initialised();
        let root = dir.path();
        let new = SocleLock::load(&SOCLE, root).unwrap();
        let mut old = new.clone();
        fs::create_dir_all(root.join(".modpack/gone")).unwrap();
        fs::write(root.join(".modpack/gone/old.sh"), b"#!/bin/sh\n").unwrap();
        old.files.push(LockedFile {
            path: ".modpack/gone/old.sh".into(),
            sha256: sha256_bytes(b"#!/bin/sh\n"),
            kind: LockedKind::File,
        });
        fs::write(
            root.join("NOTES.md"),
            "Mine.\n<!-- modpack:begin -->\nold\n<!-- modpack:end -->\n",
        )
        .unwrap();
        old.files.push(LockedFile {
            path: "NOTES.md".into(),
            sha256: sha256_bytes(b"old\n"),
            kind: LockedKind::Block,
        });

        let removed = SOCLE.remove_stale(root, &old, &new).unwrap();
        assert_eq!(removed, [".modpack/gone/old.sh", "NOTES.md"]);
        assert!(!root.join(".modpack/gone").exists());
        assert!(root.join(".modpack").is_dir());
        assert_eq!(
            fs::read_to_string(root.join("NOTES.md")).unwrap(),
            "Mine.\n"
        );
    }

    #[test]
    fn version_check_refuses_a_newer_repository() {
        let dir = initialised();
        let mut config = SocleConfig::load(&SOCLE, dir.path()).unwrap();
        check_version(dir.path()).unwrap();
        config.version = "999.0.0".into();
        config.save(&SOCLE, dir.path()).unwrap();
        let err = check_version(dir.path()).unwrap_err();
        assert!(err.to_string().contains("newer than this CLI"));
        config.version = "0.0.1".into();
        config.save(&SOCLE, dir.path()).unwrap();
        check_version(dir.path()).unwrap();
    }
}
