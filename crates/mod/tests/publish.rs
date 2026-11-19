//! The publication of a mod: the caller workflow and the pre-push hook the
//! socle lays down, and the guard of the reusable workflow they lead to. The
//! Gradle side (dry run, skipped platforms) is in `gradle_build.rs`.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};

use assert_cmd::Command;
use mc_mod::model::socle::SocleLock;
use mc_mod::socle::{HOOKS_PATH, SOCLE};

const CALLER: &str = ".github/workflows/mod-publish.yml";
const HOOK: &str = ".mod/hooks/pre-push";

/// Runs git in `dir` with a fixed identity, and returns its output.
fn git(dir: &Path, args: &[&str]) -> Output {
    Process::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Tester")
        .env("GIT_AUTHOR_EMAIL", "tester@example.com")
        .env("GIT_COMMITTER_NAME", "Tester")
        .env("GIT_COMMITTER_EMAIL", "tester@example.com")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap()
}

fn git_ok(dir: &Path, args: &[&str]) -> String {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A mod made by `mod init` in `<tmp>/mod`, committed on its version branch.
fn init_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("mod")
        .unwrap()
        .current_dir(tmp.path())
        .env_remove("RUST_LOG")
        .env("XDG_CONFIG_HOME", tmp.path().join(".machine-config"))
        .args([
            "init",
            "mod",
            "--id=swift_blades",
            "--name=Swift Blades",
            "--package=com.payangar.swiftblades",
            "--side=both-required",
            "--loaders=fabric",
            "--license=MIT",
            "--mc-version=26.3",
        ])
        .assert()
        .success();
    let root = tmp.path().join("mod");
    git_ok(&root, &["add", "-A"]);
    git_ok(&root, &["commit", "-qm", "init"]);
    (tmp, root)
}

fn template() -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("templates/socle")
            .join(CALLER),
    )
    .unwrap()
}

#[test]
fn init_lays_down_the_caller_and_enables_the_hook() {
    let (_tmp, root) = init_repo();
    assert_eq!(fs::read_to_string(root.join(CALLER)).unwrap(), template());
    let files = SocleLock::load(&SOCLE, &root).unwrap().files;
    for path in [CALLER, HOOK] {
        assert!(files.iter().any(|f| f.path == path), "{path} not recorded");
    }
    assert_eq!(
        git_ok(&root, &["config", "--get", "core.hooksPath"]),
        HOOKS_PATH
    );
}

/// The caller pins the reusable workflow to the release that embeds it, so
/// that `mod update` moves it along.
#[test]
fn the_caller_pins_this_release() {
    let tag = format!("v{}", env!("CARGO_PKG_VERSION"));
    assert!(
        template().contains(&format!(
            "uses: payangar-studio/mc-cli/.github/workflows/publish-mod.yml@{tag}\n"
        )),
        "the reusable workflow is not pinned to {tag}"
    );
}

// The pre-push hook, on pushes to a local bare remote.

/// `root` with a bare remote `origin`, its branch pushed.
fn with_remote(tmp: &Path, root: &Path) {
    let remote = tmp.join("remote.git");
    git_ok(tmp, &["init", "-q", "--bare", remote.to_str().unwrap()]);
    git_ok(root, &["remote", "add", "origin", remote.to_str().unwrap()]);
    git_ok(root, &["push", "-q", "origin", "26.3"]);
}

fn commit_changelog(root: &Path, version: &str) {
    fs::write(root.join(format!("changelogs/{version}.md")), "- Notes.\n").unwrap();
    git_ok(root, &["add", "-A"]);
    git_ok(root, &["commit", "-qm", version]);
}

fn push(root: &Path, refs: &[&str]) -> (bool, String) {
    let mut args = vec!["push", "origin"];
    args.extend_from_slice(refs);
    let out = git(root, &args);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_hook_refuses_a_tag_without_changelog() {
    let (tmp, root) = init_repo();
    with_remote(tmp.path(), &root);
    git_ok(&root, &["tag", "-a", "-m", "1.0.0", "v1.0.0"]);
    let (ok, err) = push(&root, &["v1.0.0"]);
    assert!(!ok);
    assert!(
        err.contains("tag v1.0.0 has no changelogs/1.0.0.md in its commit"),
        "{err}"
    );

    // The closest legitimate case: the notes committed, the tag moved.
    commit_changelog(&root, "1.0.0");
    git_ok(&root, &["tag", "-f", "-a", "-m", "1.0.0", "v1.0.0"]);
    let (ok, err) = push(&root, &["v1.0.0"]);
    assert!(ok, "{err}");
}

#[test]
fn the_hook_reads_the_version_of_a_tag_naming_its_minecraft_version() {
    let (tmp, root) = init_repo();
    with_remote(tmp.path(), &root);
    commit_changelog(&root, "1.1.0-beta.1");
    git_ok(&root, &["tag", "v1.1.0-beta.1+26.3"]);
    let (ok, err) = push(&root, &["v1.1.0-beta.1+26.3"]);
    assert!(ok, "{err}");
}

#[test]
fn the_hook_refuses_several_release_tags_at_once() {
    let (tmp, root) = init_repo();
    with_remote(tmp.path(), &root);
    commit_changelog(&root, "1.0.0");
    commit_changelog(&root, "1.0.1");
    git_ok(&root, &["tag", "v1.0.0", "HEAD~1"]);
    git_ok(&root, &["tag", "v1.0.1"]);
    let (ok, err) = push(&root, &["v1.0.0", "v1.0.1"]);
    assert!(!ok);
    assert!(err.contains("2 release tags in one push"), "{err}");

    // One at a time goes through; a branch push is not a release.
    for tag in ["v1.0.0", "v1.0.1"] {
        let (ok, err) = push(&root, &[tag]);
        assert!(ok, "{err}");
    }
    let (ok, err) = push(&root, &["26.3"]);
    assert!(ok, "{err}");
}

// The guard of the reusable workflow, run as its step script.

/// The `run` script of the step `name` of the reusable workflow.
fn step_script(name: &str) -> String {
    let workflow = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/publish-mod.yml"),
    )
    .unwrap();
    let mut lines = workflow
        .lines()
        .skip_while(|l| l.trim() != format!("- name: {name}"))
        .skip_while(|l| l.trim() != "run: |")
        .skip(1);
    let indent = " ".repeat(10);
    let mut script = String::new();
    for line in lines.by_ref() {
        if !line.is_empty() && !line.starts_with(&indent) {
            break;
        }
        script.push_str(line.strip_prefix(&indent).unwrap_or(line));
        script.push('\n');
    }
    assert!(!script.is_empty(), "no step `{name}`");
    script
}

/// Runs the step `Check the tag` on the commit `sha` of `root` for `tag`,
/// as the checkout leaves it: the version branch as `origin/26.3`.
fn check_tag(root: &Path, tag: &str, sha: &str) -> (bool, String, String) {
    let env = root.join("../github-env");
    let _ = fs::remove_file(&env);
    git_ok(root, &["checkout", "-q", sha]);
    let out = Process::new("bash")
        .args(["-e", "-c", &step_script("Check the tag")])
        .current_dir(root)
        .env("GITHUB_ENV", &env)
        .env("GITHUB_REF_TYPE", "tag")
        .env("GITHUB_REF", format!("refs/tags/{tag}"))
        .env("GITHUB_REF_NAME", tag)
        .env("GITHUB_SHA", sha)
        .env("DRY_RUN", "true")
        .output()
        .unwrap();
    (
        out.status.success(),
        fs::read_to_string(&env).unwrap_or_default(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// A mod whose version branch `26.3` holds the notes of 1.0.0, and a work
/// branch one commit further, with the notes of 1.1.0. Returns both commits.
fn guarded_repo() -> (tempfile::TempDir, PathBuf, String, String) {
    let (tmp, root) = init_repo();
    commit_changelog(&root, "1.0.0");
    let on_branch = git_ok(&root, &["rev-parse", "HEAD"]);
    git_ok(
        &root,
        &["update-ref", "refs/remotes/origin/26.3", &on_branch],
    );
    git_ok(&root, &["checkout", "-qb", "work"]);
    commit_changelog(&root, "1.1.0");
    let off_branch = git_ok(&root, &["rev-parse", "HEAD"]);
    (tmp, root, on_branch, off_branch)
}

#[test]
fn the_guard_accepts_a_tag_on_its_version_branch() {
    let (_tmp, root, on_branch, _) = guarded_repo();
    for tag in ["v1.0.0", "v1.0.0+26.3"] {
        let (ok, env, out) = check_tag(&root, tag, &on_branch);
        assert!(ok, "{tag}: {out}");
        assert_eq!(env, "VERSION=1.0.0\n", "{tag}");
    }
}

#[test]
fn the_guard_refuses_a_tag_outside_its_version_branch() {
    let (_tmp, root, _, off_branch) = guarded_repo();
    let (ok, env, out) = check_tag(&root, "v1.1.0", &off_branch);
    assert!(!ok);
    assert!(env.is_empty());
    assert!(
        out.contains("Tag v1.1.0 is not on its version branch"),
        "{out}"
    );
}

#[test]
fn the_guard_refuses_a_tag_without_changelog() {
    let (_tmp, root, on_branch, _) = guarded_repo();
    let (ok, _, out) = check_tag(&root, "v1.0.1", &on_branch);
    assert!(!ok);
    assert!(
        out.contains("Tag v1.0.1 has no changelogs/1.0.1.md"),
        "{out}"
    );
}

#[test]
fn the_guard_refuses_a_tag_naming_another_minecraft_version_or_no_version() {
    let (_tmp, root, on_branch, _) = guarded_repo();
    let (ok, _, out) = check_tag(&root, "v1.0.0+26.4", &on_branch);
    assert!(!ok);
    assert!(
        out.contains("names Minecraft 26.4, but its commit is a Minecraft 26.3 mod"),
        "{out}"
    );
    let (ok, _, out) = check_tag(&root, "vnext", &on_branch);
    assert!(!ok);
    assert!(out.contains("Tag vnext is not a release tag"), "{out}");
}
