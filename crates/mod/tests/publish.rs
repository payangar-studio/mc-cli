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

/// The `mod` binary run in `dir`, with the machine configuration and git's
/// global configuration (`<tmp>/.gitconfig`, absent unless a test writes it)
/// under `tmp`.
fn mod_cmd(tmp: &Path, dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("mod").unwrap();
    cmd.current_dir(dir)
        .env_remove("RUST_LOG")
        .env("XDG_CONFIG_HOME", tmp.join(".machine-config"))
        .env("GIT_CONFIG_GLOBAL", tmp.join(".gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1");
    cmd
}

/// A mod made by `mod init` in `<tmp>/mod`, committed on its version branch.
fn init_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = init_in(tmp.path());
    (tmp, root)
}

/// `mod init` in `<tmp>/mod`, committed on its version branch.
fn init_in(tmp: &Path) -> PathBuf {
    mod_cmd(tmp, tmp)
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
    let root = tmp.join("mod");
    git_ok(&root, &["add", "-A"]);
    git_ok(&root, &["commit", "-qm", "init"]);
    root
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

/// A clone has no local `core.hooksPath`: the next `mod` command in the
/// repository enables the hooks.
#[test]
fn update_enables_the_hook_of_a_clone() {
    let (tmp, root) = init_repo();
    git_ok(&root, &["config", "--unset", "core.hooksPath"]);
    mod_cmd(tmp.path(), &root).arg("update").assert().success();
    assert_eq!(
        git_ok(&root, &["config", "--get", "core.hooksPath"]),
        HOOKS_PATH
    );
}

/// A `core.hooksPath` set in git's global configuration (a hooks manager) is
/// kept, with a warning, rather than overridden for the mod.
#[test]
fn init_keeps_a_global_hooks_path() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join(".gitconfig"),
        "[core]\n\thooksPath = /shared/hooks\n",
    )
    .unwrap();
    let out = mod_cmd(tmp.path(), tmp.path())
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
        .success()
        .get_output()
        .clone();
    let root = tmp.path().join("mod");
    let local = git(&root, &["config", "--local", "--get", "core.hooksPath"]);
    assert!(
        !local.status.success(),
        "core.hooksPath set locally: {}",
        String::from_utf8_lossy(&local.stdout)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("core.hooksPath is `/shared/hooks`"), "{err}");
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
    commit_notes(root, version, "- Notes.\n");
}

fn commit_notes(root: &Path, version: &str, notes: &str) {
    fs::write(root.join(format!("changelogs/{version}.md")), notes).unwrap();
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
fn the_hook_refuses_a_changelog_without_notes() {
    let (tmp, root) = init_repo();
    with_remote(tmp.path(), &root);
    commit_notes(&root, "1.0.0", "\n  \n");
    git_ok(&root, &["tag", "v1.0.0"]);
    let (ok, err) = push(&root, &["v1.0.0"]);
    assert!(!ok);
    assert!(
        err.contains("tag v1.0.0 has no changelogs/1.0.0.md in its commit"),
        "{err}"
    );
}

/// GitHub starts no workflow for a push of more than three tags: the hook
/// refuses such a push when it carries a release tag, and lets the others go.
#[test]
fn the_hook_refuses_a_release_tag_among_more_than_three_tags() {
    let (tmp, root) = init_repo();
    with_remote(tmp.path(), &root);
    for version in ["1.0.0", "1.0.1", "1.0.2"] {
        commit_changelog(&root, version);
        git_ok(&root, &["tag", &format!("v{version}")]);
    }
    for tag in ["a", "b", "c", "d"] {
        git_ok(&root, &["tag", tag]);
    }
    let (ok, err) = push(&root, &["v1.0.2", "a", "b", "c"]);
    assert!(!ok);
    assert!(err.contains("4 tags in one push"), "{err}");

    // Up to three tags start their workflows; tags without a release do
    // not publish.
    let (ok, err) = push(&root, &["v1.0.0", "v1.0.1", "v1.0.2"]);
    assert!(ok, "{err}");
    let (ok, err) = push(&root, &["a", "b", "c", "d"]);
    assert!(ok, "{err}");
    let (ok, err) = push(&root, &["26.3"]);
    assert!(ok, "{err}");
}

/// A tag starting with `v` that is no version is not a release: the hook
/// looks for no changelog.
#[test]
fn the_hook_ignores_a_tag_that_is_not_a_release() {
    let (tmp, root) = init_repo();
    with_remote(tmp.path(), &root);
    git_ok(&root, &["tag", "vendor-sync"]);
    let (ok, err) = push(&root, &["vendor-sync"]);
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
/// as the checkout leaves it: the version branch as `origin/26.3`. Returns
/// the step's outputs.
fn check_tag(root: &Path, tag: &str, sha: &str) -> (bool, String, String) {
    let env = root.join("../github-output");
    let _ = fs::remove_file(&env);
    git_ok(root, &["checkout", "-q", sha]);
    let out = Process::new("bash")
        .args(["-e", "-c", &step_script("Check the tag")])
        .current_dir(root)
        .env("GITHUB_OUTPUT", &env)
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
        assert_eq!(env, "version=1.0.0\n", "{tag}");
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
fn the_guard_refuses_a_changelog_without_notes() {
    let (_tmp, root) = init_repo();
    commit_notes(&root, "1.0.0", "\n  \n");
    let sha = git_ok(&root, &["rev-parse", "HEAD"]);
    git_ok(&root, &["update-ref", "refs/remotes/origin/26.3", &sha]);
    let (ok, _, out) = check_tag(&root, "v1.0.0", &sha);
    assert!(!ok);
    assert!(
        out.contains("Tag v1.0.0 has no changelogs/1.0.0.md"),
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

// The vault step of the reusable workflow, against a fake vault: `curl` is a
// shell function answering each endpoint, `jq` is the real one.

const FAKE_CURL: &str = r#"
curl() {
  local url="${!#}"
  case "$url" in
    */oidc-auth/login)
      cat > /dev/null
      if [[ -n "$FAKE_LOGIN_FAILS" ]]; then
        echo "curl: (22) The requested URL returned error: 401" >&2
        return 22
      fi
      echo '{"accessToken": "vault-token"}' ;;
    */id-token) echo '{"value": "oidc-jwt"}' ;;
    */api/v1/workspace) echo '{"workspaces": [{"name": "minecraft", "slug": "minecraft-a1b2"}]}' ;;
    */api/v3/secrets/raw) echo "$FAKE_SECRETS" ;;
    *) echo "unexpected curl $url" >&2; return 1 ;;
  esac
}
"#;

/// Runs the step `Read the secrets from the vault` with the vault holding
/// `secrets` (name, value), and returns its success, the variables it
/// exported and its stdout.
fn read_vault(secrets: &[(&str, &str)], login_fails: bool) -> (bool, String, String) {
    let tmp = tempfile::tempdir().unwrap();
    let env = tmp.path().join("github-env");
    let secrets = serde_json::json!({
        "secrets": secrets
            .iter()
            .map(|(k, v)| serde_json::json!({"secretKey": k, "secretValue": v}))
            .collect::<Vec<_>>()
    });
    let script = format!(
        "{FAKE_CURL}\n{}",
        step_script("Read the secrets from the vault")
    );
    let out = Process::new("bash")
        .args(["-e", "-c", &script])
        .current_dir(tmp.path())
        .env("GITHUB_ENV", &env)
        .env("VAULT_URL", "https://vault.test/")
        .env("IDENTITY_ID", "identity")
        .env("VAULT_PROJECT", "minecraft")
        .env("VAULT_ENVIRONMENT", "prod")
        .env("VAULT_PATH", "/app")
        .env("USER_AGENT", "test")
        .env(
            "ACTIONS_ID_TOKEN_REQUEST_URL",
            "https://actions.test/id-token",
        )
        .env("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "request-token")
        .env("GITHUB_REPOSITORY_OWNER", "payangar-studio")
        .env("GITHUB_REPOSITORY", "payangar-studio/swift-blades")
        .env("GITHUB_REF", "refs/tags/v1.0.0")
        .env("FAKE_SECRETS", secrets.to_string())
        .env("FAKE_LOGIN_FAILS", if login_fails { "1" } else { "" })
        .output()
        .unwrap();
    (
        out.status.success(),
        fs::read_to_string(&env).unwrap_or_default(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn the_vault_step_exports_both_tokens() {
    let (ok, env, out) = read_vault(
        &[
            ("MC_MODRINTH_TOKEN", "modrinth-token"),
            ("MC_CURSEFORGE_UPLOAD_TOKEN", "curseforge-token"),
        ],
        false,
    );
    assert!(ok, "{out}");
    assert_eq!(
        env,
        "MC_MODRINTH_TOKEN=modrinth-token\nMC_CURSEFORGE_UPLOAD_TOKEN=curseforge-token\n"
    );
}

/// A missing token stops the job before anything is published: with one
/// token only, the build would publish on one platform and fail on the other.
#[test]
fn the_vault_step_refuses_a_missing_token() {
    let (ok, _, out) = read_vault(&[("MC_MODRINTH_TOKEN", "modrinth-token")], false);
    assert!(!ok, "{out}");
    assert!(
        out.contains("::error::MC_CURSEFORGE_UPLOAD_TOKEN is not in minecraft:/app"),
        "{out}"
    );
}

#[test]
fn the_vault_step_reports_a_refused_login() {
    let (ok, env, out) = read_vault(
        &[
            ("MC_MODRINTH_TOKEN", "modrinth-token"),
            ("MC_CURSEFORGE_UPLOAD_TOKEN", "curseforge-token"),
        ],
        true,
    );
    assert!(!ok, "{out}");
    assert!(env.is_empty(), "{env}");
    assert!(
        out.contains("::error::The vault refused the login"),
        "{out}"
    );
}
