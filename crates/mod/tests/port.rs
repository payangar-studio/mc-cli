//! `mod port`: a generated mod moved to another Minecraft version, against a
//! fixture version table, and the refusals that leave the repository as it was.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use assert_cmd::Command;
use mc_mod::cli::port::port;
use mc_mod::error::ErrorKind;
use mc_mod::model::socle::SocleConfig;
use mc_mod::socle::SOCLE;
use mc_mod::versions::Pinned;
use predicates::prelude::*;

/// A version table with the version mods are generated on and a later one.
const TABLE: &[Pinned] = &[
    Pinned {
        minecraft: "26.4",
        fabric_loader: "0.21.0",
        fabric_api: "0.170.0+26.4",
        loom: "1.20.1",
        neoforge: "26.4.0.7",
        moddevgradle: "2.0.150",
    },
    Pinned {
        minecraft: "26.3",
        fabric_loader: "0.20.1",
        fabric_api: "0.166.2+26.3",
        loom: "1.19.4",
        neoforge: "26.3.0.113",
        moddevgradle: "2.0.148",
    },
];

/// The binary without a terminal, with a machine configuration of its own.
fn bin(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("mod").unwrap();
    cmd.current_dir(dir)
        .env_remove("RUST_LOG")
        .env("XDG_CONFIG_HOME", dir.join(".machine-config"));
    cmd
}

/// Runs git in `dir`, which must succeed; its trimmed stdout.
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
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
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

#[test]
fn port_creates_the_version_branch_with_its_versions() {
    let (_tmp, root) = committed_mod();
    let before = SocleConfig::load(&SOCLE, &root).unwrap();
    let head = git(&root, &["rev-parse", "HEAD"]);

    let ported = port(&root, "26.4", TABLE).unwrap();

    assert_eq!(ported.from_branch, "26.3");
    assert_eq!(ported.from, before.init);
    assert_eq!(ported.to, TABLE[0].versions());
    // The new branch starts from the old one, which is left as it was.
    assert_eq!(git(&root, &["branch", "--show-current"]), "26.4");
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&root, &["rev-parse", "26.3"]), head);
    // Only the socle settings change: the versions, nothing else.
    assert_eq!(git(&root, &["status", "--porcelain"]), "M .mod/socle.toml");
    let after = SocleConfig::load(&SOCLE, &root).unwrap();
    assert_eq!(after.version, before.version);
    assert_eq!(after.init, TABLE[0].versions());
    let text = fs::read_to_string(root.join(".mod/socle.toml")).unwrap();
    assert!(text.contains("minecraft = \"26.4\""), "{text}");
    assert!(text.contains("fabric-api = \"0.170.0+26.4\""), "{text}");
    assert!(text.contains("neoforge = \"26.4.0.7\""), "{text}");
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
fn the_cli_ports_with_its_own_table() {
    let (_tmp, root) = committed_mod();

    // The CLI's table holds 26.3, the mod's current version.
    bin(&root)
        .args(["port", "26.3"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already on Minecraft 26.3"));
    assert_eq!(git(&root, &["branch", "--list"]), "* 26.3");
}

#[test]
fn uncommitted_changes_are_refused() {
    let (_tmp, root) = committed_mod();
    fs::write(root.join("notes.txt"), "wip\n").unwrap();

    let err = port(&root, "26.4", TABLE).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Usage);
    assert!(err.to_string().contains("uncommitted changes"), "{err}");
    assert!(err.to_string().contains("notes.txt"), "{err}");
    assert_eq!(git(&root, &["branch", "--list"]), "* 26.3");
}

#[test]
fn an_existing_branch_is_refused() {
    let (_tmp, root) = committed_mod();
    git(&root, &["branch", "26.4"]);

    let err = port(&root, "26.4", TABLE).unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Usage);
    assert!(
        err.to_string().contains("branch 26.4 already exists"),
        "{err}"
    );
    assert_eq!(git(&root, &["branch", "--show-current"]), "26.3");
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
