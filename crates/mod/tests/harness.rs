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
use yaml_rust2::YamlLoader;

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
    init(tmp.path(), loaders, side).success();
    let root = tmp.path().join("swift-blades");
    (tmp, root)
}

/// Runs `mod init` for `<dir>/swift-blades`.
fn init(dir: &Path, loaders: &str, side: &str) -> assert_cmd::assert::Assert {
    bin(dir)
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
}

/// Runs git in `dir`, which must succeed.
fn git(dir: &Path, args: &[&str]) {
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
}

/// Runs the step of `mod-agents-md.yml` that checks AGENTS.md, as GitHub
/// would for an event whose `github` context holds `context`: its `env`
/// expressions evaluated (`a || b`), then its `run` script.
fn run_check_step(root: &Path, context: &[(&str, &str)]) -> Output {
    let text = fs::read_to_string(root.join(".github/workflows/mod-agents-md.yml")).unwrap();
    let doc = &YamlLoader::load_from_str(&text).unwrap()[0];
    let step = doc["jobs"]["check-agents-md"]["steps"]
        .as_vec()
        .unwrap()
        .iter()
        .find(|s| s["run"].as_str().is_some())
        .unwrap();
    let lookup = |name: &str| {
        let name = name.trim().strip_prefix("github.").unwrap();
        context
            .iter()
            .find(|(k, _)| *k == name)
            .map_or("", |(_, v)| *v)
    };
    let mut cmd = Process::new("bash");
    cmd.arg("-c")
        .arg(step["run"].as_str().unwrap())
        .current_dir(root);
    for (name, value) in step["env"].as_hash().into_iter().flatten() {
        let mut value = value.as_str().unwrap().to_owned();
        while let Some(start) = value.find("${{") {
            let end = start + value[start..].find("}}").unwrap();
            let result = value[start + 3..end]
                .split("||")
                .map(lookup)
                .find(|v| !v.is_empty())
                .unwrap_or("");
            value.replace_range(start..end + 2, result);
        }
        cmd.env(name.as_str().unwrap(), value);
    }
    cmd.output().unwrap()
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

/// An older version branch grows its own AGENTS.md section over several small
/// pushes: each push is checked on its own, not against the default branch.
#[test]
fn a_push_to_a_version_branch_is_not_measured_against_the_default_branch() {
    let (tmp, root) = init_repo("fabric", "client");
    let origin = tmp.path().join("origin.git");
    git(tmp.path(), &["init", "-q", "--bare", "origin.git"]);
    git(&root, &["init", "-q", "-b", "26.3"]);
    git(
        &root,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "init"]);
    git(&root, &["push", "-q", "origin", "26.3"]);
    git(&root, &["switch", "-q", "-c", "1.21.1"]);
    let rule = format!(
        "- {}\n",
        "Blades of this version keep their legacy durability. ".repeat(5)
    );
    let mut agents = fs::read_to_string(root.join("AGENTS.md")).unwrap();
    agents.push_str("\n## Minecraft 1.21.1\n\n");
    for n in 0..6 {
        agents.push_str(&rule);
        fs::write(root.join("AGENTS.md"), &agents).unwrap();
        git(&root, &["commit", "-q", "-am", &format!("rule {n}")]);
        git(&root, &["push", "-q", "origin", "1.21.1"]);
    }

    let push = run_check_step(
        &root,
        &[
            ("ref_name", "1.21.1"),
            ("event.repository.default_branch", "26.3"),
        ],
    );
    assert!(
        push.status.success(),
        "{}{}",
        String::from_utf8_lossy(&push.stdout),
        String::from_utf8_lossy(&push.stderr)
    );

    // The same lines in one pull request into the default branch exceed the
    // amendment limits.
    let pull_request = run_check_step(
        &root,
        &[
            ("base_ref", "26.3"),
            ("head_ref", "1.21.1"),
            ("event.repository.default_branch", "26.3"),
        ],
    );
    assert_eq!(pull_request.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&pull_request.stderr).contains("1500"),
        "{}",
        String::from_utf8_lossy(&pull_request.stderr)
    );
}

#[test]
fn a_claude_md_of_its_own_is_flagged() {
    for (claude, flagged) in [("# Notes of my own\n", true), ("@AGENTS.md\n", false)] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("swift-blades");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("CLAUDE.md"), claude).unwrap();

        let assert = init(tmp.path(), "fabric", "client").success();

        let err = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        assert_eq!(
            err.contains("CLAUDE.md is not a link to AGENTS.md"),
            flagged,
            "{claude}: {err}"
        );
        assert_eq!(fs::read_to_string(root.join("CLAUDE.md")).unwrap(), claude);
    }
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
fn setup_names_a_remote_it_cannot_read() {
    let (_tmp, root) = init_repo("fabric", "client");
    git(&root, &["init", "-q"]);
    let url = "git@github-work:payangar-studio/swift-blades.git";
    git(&root, &["remote", "add", "origin", url]);
    bin(&root)
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .arg("setup")
        .assert()
        .success()
        .stdout(predicate::str::contains(url))
        .stdout(predicate::str::contains("No GitHub remote").not());
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
