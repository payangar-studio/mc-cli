//! Integration tests of the project type guard: `modpack` refuses to work in
//! a repository of another type.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

/// Every command that works in a repository, with the arguments it needs to
/// get past parsing. `init` and `auth` do not, and are not guarded.
const COMMANDS: &[&[&str]] = &[
    &["setup"],
    &["update"],
    &["add", "sodium"],
    &["remove", "sodium"],
    &["manifest"],
    &["install"],
    &["variant", "list"],
    &["variant", "load", "server"],
    &["status"],
    &["test"],
    &["pack"],
    &["page", "push"],
    &["approve", "--check"],
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn modpack(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("modpack").unwrap();
    cmd.current_dir(dir).env_remove("RUST_LOG").env_remove("CI");
    cmd
}

#[test]
fn every_command_is_refused_in_a_mod_repository() {
    let dir = fixtures().join("mod/src");
    for words in COMMANDS {
        modpack(&dir)
            .args(*words)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("is a mod repository"))
            .stderr(predicate::str::contains("use the `mod` binary here"));
    }
}

#[test]
fn a_command_works_from_a_subdirectory_of_a_modpack_repository() {
    modpack(&fixtures().join("repo/config/nested"))
        .args(["test", "server", "--condition", "side"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 passed, 0 failed"));
}

#[test]
fn init_refuses_the_root_of_a_mod_repository() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".mod")).unwrap();
    fs::write(dir.path().join(".mod/socle.toml"), "").unwrap();
    modpack(dir.path())
        .arg("init")
        .args([
            "--name=Test Pack",
            "--mc-version=1.21.1",
            "--loader=neoforge",
            "--loader-version=21.1.77",
            "--author=Tester",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("is a mod repository"))
        .stderr(predicate::str::contains("use the `mod` binary here"));
    assert!(!dir.path().join(".modpack").exists());
}
