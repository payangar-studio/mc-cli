//! Integration tests of the project type guard: `mod` refuses to work in a
//! repository of another type.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

/// Every command that works in a repository. `init` is not guarded.
const COMMANDS: &[&[&str]] = &[&["update"], &["port", "26.3"]];

fn bin(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("mod").unwrap();
    cmd.current_dir(dir)
        .env_remove("RUST_LOG")
        .env("XDG_CONFIG_HOME", dir.join(".machine-config"));
    cmd
}

fn modpack_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".modpack")).unwrap();
    fs::write(dir.path().join(".modpack/socle.toml"), "").unwrap();
    fs::create_dir_all(dir.path().join("config")).unwrap();
    dir
}

#[test]
fn every_command_is_refused_in_a_modpack_repository() {
    let dir = modpack_repo();
    for words in COMMANDS {
        bin(&dir.path().join("config"))
            .args(*words)
            .assert()
            .code(2)
            .stderr(predicate::str::contains("is a modpack repository"))
            .stderr(predicate::str::contains("use the `modpack` binary here"));
    }
}

#[test]
fn a_command_outside_a_repository_points_at_init() {
    let dir = tempfile::tempdir().unwrap();
    bin(dir.path())
        .arg("update")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("mod init"));
}

#[test]
fn init_refuses_the_root_of_a_modpack_repository() {
    let dir = modpack_repo();
    bin(dir.path())
        .arg("init")
        .args([
            "--id=swift_blades",
            "--name=Swift Blades",
            "--package=com.payangar.swiftblades",
            "--side=client",
            "--loaders=fabric",
            "--license=MIT",
            "--mc-version=26.3",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("is a modpack repository"));
    assert!(!dir.path().join(".mod").exists());
}
