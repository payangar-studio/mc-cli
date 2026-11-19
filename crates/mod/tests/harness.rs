//! The harness a generated mod gets: the `AGENTS.md` block and its check, and
//! `mod setup` without GitHub. The GitHub API side is tested against a mock
//! in `src/github/tests.rs`; the workflow templates in `workflow_templates.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};

use assert_cmd::Command;
use mc_mod::model::socle::{LockedKind, SocleLock};
use mc_mod::socle::SOCLE;
use predicates::prelude::*;

/// The binary without a terminal, with a machine configuration of its own.
fn bin(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("mod").unwrap();
    cmd.current_dir(dir)
        .env_remove("RUST_LOG")
        .env("XDG_CONFIG_HOME", dir.join(".machine-config"));
    cmd
}

/// A fresh mod with these loaders and side, in `<tmp>/swift-blades`.
fn init_repo(loaders: &str, side: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    bin(tmp.path())
        .args(["init", "swift-blades"])
        .args([
            "--id=swift_blades",
            "--name=Swift Blades",
            "--package=com.payangar.swiftblades",
            "--license=MIT",
            "--mc-version=26.3",
        ])
        .arg(format!("--loaders={loaders}"))
        .arg(format!("--side={side}"))
        .assert()
        .success();
    let root = tmp.path().join("swift-blades");
    (tmp, root)
}

/// Runs the socle's AGENTS.md check in `root`, as CI does.
fn check_agents_md(root: &Path) -> Output {
    Process::new("node")
        .arg(".mod/scripts/check-agents-md.mjs")
        .current_dir(root)
        .output()
        .expect("node runs the AGENTS.md check: install Node.js")
}

#[test]
fn a_generated_mod_has_the_block_and_its_check_passes() {
    for (loaders, side) in [
        ("fabric,neoforge", "both-required"),
        ("fabric", "client"),
        ("neoforge", "server"),
    ] {
        let (_tmp, root) = init_repo(loaders, side);
        let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert!(
            agents.starts_with("<!-- mod:begin -->\n## Mod socle\n"),
            "{agents}"
        );
        assert!(agents.contains("`mod update`"), "{agents}");
        assert!(
            !agents.contains("modpack"),
            "the block cites only its binary"
        );
        let claude = fs::read_link(root.join("CLAUDE.md")).unwrap();
        assert_eq!(claude, Path::new("AGENTS.md"));
        for path in [
            ".claude/skills/editing-agents-md/SKILL.md",
            ".mod/scripts/check-agents-md.mjs",
            ".github/workflows/mod-agents-md.yml",
        ] {
            assert!(root.join(path).is_file(), "{path} missing");
        }

        let out = check_agents_md(&root);
        assert!(
            out.status.success(),
            "{loaders} {side}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn a_broken_path_in_agents_md_fails_the_check() {
    let (_tmp, root) = init_repo("fabric,neoforge", "both-required");
    let mut agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    agents.push_str(
        "\n## Notes\n\n- The tests of the blades are in `common/src/test/java/Blades.java`.\n",
    );
    fs::write(root.join("AGENTS.md"), agents).unwrap();

    let out = check_agents_md(&root);

    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("`common/src/test/java/Blades.java` does not exist"),
        "{err}"
    );
}

#[test]
fn the_block_is_registered_and_kept_by_update() {
    let (_tmp, root) = init_repo("fabric", "client");
    let lock = SocleLock::load(&SOCLE, &root).unwrap();
    assert!(
        lock.files
            .iter()
            .any(|f| f.path == "AGENTS.md" && f.kind == LockedKind::Block)
    );
    // The mod's own instructions, under the block, survive an update.
    let mut agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    agents.push_str("\n## Swift Blades\n\n- Blades never break.\n");
    fs::write(root.join("AGENTS.md"), &agents).unwrap();
    bin(&root).args(["update", "--force"]).assert().success();
    assert_eq!(fs::read_to_string(root.join("AGENTS.md")).unwrap(), agents);
}

#[test]
fn setup_without_github_changes_nothing() {
    let (_tmp, root) = init_repo("fabric", "client");
    bin(&root)
        .args(["setup", "--no-github"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Skipped the GitHub repository"));
    assert!(!root.join(".github/CODEOWNERS").exists());
}

#[test]
fn setup_without_a_github_remote_says_what_to_do() {
    let (_tmp, root) = init_repo("fabric", "client");
    bin(&root)
        // No token either: setup must stop before needing one.
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .arg("setup")
        .assert()
        .success()
        .stdout(predicate::str::contains("No GitHub remote `origin`"))
        .stdout(predicate::str::contains("run `mod setup` again"));
}

#[test]
fn setup_refuses_outside_a_mod() {
    let tmp = tempfile::tempdir().unwrap();
    bin(tmp.path())
        .args(["setup", "--no-github"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not inside a mod repository"));
}
