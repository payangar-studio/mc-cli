//! `mod port` through the binary: a generated mod moved to the CLI's newest
//! version, and the refusals that leave the repository as it was. The port to
//! a fixture version table is tested in the module (`src/cli/port.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use assert_cmd::Command;
use mc_mod::model::socle::SocleConfig;
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

/// Runs git in `dir`, which must succeed; its stdout without the trailing
/// newline.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Process::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
}

/// A mod generated on 26.3, committed on its branch `26.3`.
fn committed_mod() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    bin(tmp.path())
        .args(["init", "swift-blades"])
        .args([
            "--id=swift_blades",
            "--name=Swift Blades",
            "--package=com.payangar.swiftblades",
            "--side=both-required",
            "--loaders=fabric,neoforge",
            "--license=MIT",
            "--mc-version=26.3",
        ])
        .assert()
        .success();
    let root = tmp.path().join("swift-blades");
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "init"]);
    assert_eq!(git(&root, &["branch", "--show-current"]), "26.3");
    (tmp, root)
}

/// The committed mod, its socle settings changed by `edit` and committed on
/// the branch `branch`.
fn edited_mod(branch: &str, edit: impl FnOnce(&mut SocleConfig)) -> (tempfile::TempDir, PathBuf) {
    let (tmp, root) = committed_mod();
    let mut config = SocleConfig::load(&SOCLE, &root).unwrap();
    edit(&mut config);
    config.save(&SOCLE, &root).unwrap();
    git(&root, &["branch", "-m", branch]);
    git(&root, &["commit", "-q", "-am", "edit"]);
    (tmp, root)
}

/// The committed mod as if it were on Minecraft 26.2, a version before the
/// CLI's 26.3.
fn mod_on_26_2() -> (tempfile::TempDir, PathBuf) {
    edited_mod("26.2", |config| {
        config.init.minecraft = "26.2".into();
        config.init.fabric_api = "0.150.0+26.2".into();
    })
}

#[test]
fn port_creates_the_branch_and_lists_the_changed_versions() {
    let (_tmp, root) = mod_on_26_2();
    let head = git(&root, &["rev-parse", "HEAD"]);

    bin(&root)
        .args(["port", "26.3"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "Created branch 26.3 from 26.2, with these versions in .mod/socle.toml:\n  minecraft: 26.2 -> 26.3\n  fabric-api: 0.150.0+26.2 -> 0.166.2+26.3\nNothing is committed.",
        ));

    assert_eq!(git(&root, &["branch", "--show-current"]), "26.3");
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&root, &["rev-parse", "26.2"]), head);
    assert_eq!(git(&root, &["status", "--porcelain"]), " M .mod/socle.toml");
}

/// The table of the CLI only knows released versions: an unknown one is a
/// usage error that leaves the repository untouched.
#[test]
fn an_unknown_version_says_to_update_mc_cli() {
    let (_tmp, root) = committed_mod();

    bin(&root)
        .args(["port", "1.21.1"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "Minecraft 1.21.1 is not supported by this mc-cli",
        ))
        .stderr(predicate::str::contains("26.3"))
        .stderr(predicate::str::contains("update mc-cli"));

    assert_eq!(git(&root, &["branch", "--list"]), "* 26.3");
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
}

#[test]
fn the_version_the_mod_is_on_is_refused() {
    let (_tmp, root) = committed_mod();

    bin(&root)
        .args(["port", "26.3"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already on Minecraft 26.3"));
    assert_eq!(git(&root, &["branch", "--list"]), "* 26.3");
}

/// The versions of the CLI's table go with the build of its own socle: an
/// older `build-logic/` might not build them, so `mod update` comes first.
#[test]
fn an_older_socle_is_refused_until_mod_update() {
    let (_tmp, root) = edited_mod("26.2", |config| {
        config.init.minecraft = "26.2".into();
        config.version = "0.0.1".into();
    });

    bin(&root)
        .args(["port", "26.3"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "error: this repository uses socle 0.0.1",
        ))
        .stderr(predicate::str::contains(
            "run `mod update`, commit, then rerun `mod port 26.3`",
        ));

    assert_eq!(git(&root, &["branch", "--list"]), "* 26.2");
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
}

#[test]
fn uncommitted_changes_are_refused() {
    let (_tmp, root) = mod_on_26_2();
    fs::write(root.join("mod.toml"), "changed\n").unwrap();
    fs::write(root.join("notes.txt"), "wip\n").unwrap();

    bin(&root)
        .args(["port", "26.3"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "uncommitted changes:\n M mod.toml\n?? notes.txt",
        ));

    assert_eq!(git(&root, &["branch", "--list"]), "* 26.2");
}

#[test]
fn an_existing_branch_is_refused() {
    let (_tmp, root) = mod_on_26_2();
    git(&root, &["branch", "26.3"]);

    bin(&root)
        .args(["port", "26.3"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("branch 26.3 already exists"));

    assert_eq!(git(&root, &["branch", "--show-current"]), "26.2");
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
}

#[test]
fn the_skill_is_posed_and_cited_in_the_agents_block() {
    let (_tmp, root) = committed_mod();
    let skill = fs::read_to_string(root.join(".claude/skills/mod-port/SKILL.md")).unwrap();
    assert!(skill.starts_with("---\nname: mod-port\n"), "{skill}");
    let agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(agents.contains("`mod port <version>`"), "{agents}");
    assert!(agents.contains("`mod-port` skill"), "{agents}");
}
