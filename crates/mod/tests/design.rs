//! The `design` branch `mod init` creates and keeps local (`mod setup`
//! publishes it, see `crates/mod/src/github/tests.rs`), and the `mod-design`
//! skill with the check of its output.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use assert_cmd::Command;
use mc_mod::model::socle::SocleLock;
use mc_mod::socle::SOCLE;
use predicates::prelude::*;

const INIT_ARGS: &[&str] = &[
    "--id=swift_blades",
    "--name=Swift Blades",
    "--package=com.payangar.swiftblades",
    "--side=both-required",
    "--loaders=fabric,neoforge",
    "--license=MIT",
    "--mc-version=26.3",
];

const SKILL: &str = ".claude/skills/mod-design/SKILL.md";
const CHECK: &str = ".claude/skills/mod-design/check-page.sh";

/// The binary without a terminal, with a machine configuration and a git
/// identity of its own.
fn bin(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("mod").unwrap();
    cmd.current_dir(dir)
        .env_remove("RUST_LOG")
        .env("XDG_CONFIG_HOME", dir.join(".machine-config"));
    for (var, value) in identity() {
        cmd.env(var, value);
    }
    cmd
}

fn identity() -> [(&'static str, &'static str); 4] {
    [
        ("GIT_AUTHOR_NAME", "Tester"),
        ("GIT_AUTHOR_EMAIL", "tester@example.invalid"),
        ("GIT_COMMITTER_NAME", "Tester"),
        ("GIT_COMMITTER_EMAIL", "tester@example.invalid"),
    ]
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Process::new("git")
        .args(args)
        .current_dir(dir)
        .envs(identity())
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A fresh mod in `<tmp>/swift-blades`.
fn init_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    bin(tmp.path())
        .arg("init")
        .arg("swift-blades")
        .args(INIT_ARGS)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created the `design` branch"));
    let root = tmp.path().join("swift-blades");
    (tmp, root)
}

#[test]
fn init_creates_an_orphan_design_branch_without_touching_the_checkout() {
    let (_tmp, root) = init_repo();
    let files = git(&root, &["ls-tree", "-r", "--name-only", "design"]);
    assert_eq!(
        files.lines().collect::<Vec<_>>(),
        [
            "docs/.nojekyll",
            "docs/design/00-overview.md",
            "docs/design/index.html"
        ]
    );
    // One commit, without parent.
    assert_eq!(git(&root, &["rev-list", "--count", "design"]), "1");
    assert_eq!(
        git(&root, &["rev-list", "--parents", "design"])
            .split(' ')
            .count(),
        1
    );
    let page = git(&root, &["show", "design:docs/design/index.html"]);
    assert!(page.contains("<h1>Swift Blades</h1>"), "{page}");
    let overview = git(&root, &["show", "design:docs/design/00-overview.md"]);
    assert!(overview.contains("`both-required`"));

    // The version branch, its index and its files are as init leaves them.
    assert_eq!(git(&root, &["branch", "--show-current"]), "26.3");
    assert_eq!(git(&root, &["ls-files"]), "");
    assert!(!root.join("docs").exists());
}

#[test]
fn init_keeps_an_existing_design_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    git(root, &["init", "--quiet", "--initial-branch=26.3"]);
    git(root, &["checkout", "--quiet", "--orphan", "design"]);
    git(root, &["commit", "--quiet", "--allow-empty", "-m", "mine"]);
    git(root, &["checkout", "--quiet", "--orphan", "26.3"]);
    let before = git(root, &["rev-parse", "design"]);

    bin(root)
        .arg("init")
        .args(INIT_ARGS)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Kept the existing `design` branch",
        ));
    assert_eq!(git(root, &["rev-parse", "design"]), before);
}

/// Without a git identity the branch cannot be committed: init stops before
/// the socle, so that it resumes once the identity is set.
#[test]
fn init_without_a_git_identity_stops_before_the_socle() {
    let tmp = tempfile::tempdir().unwrap();
    let empty = tmp.path().join("gitconfig");
    fs::write(&empty, "").unwrap();
    let mut cmd = Command::cargo_bin("mod").unwrap();
    cmd.current_dir(tmp.path())
        .env_remove("RUST_LOG")
        .env("XDG_CONFIG_HOME", tmp.path().join(".machine-config"))
        .env("GIT_CONFIG_GLOBAL", &empty)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "user.useConfigOnly")
        .env("GIT_CONFIG_VALUE_0", "true");
    for (var, _) in identity() {
        cmd.env_remove(var);
    }
    cmd.env_remove("EMAIL");
    cmd.arg("init")
        .args(INIT_ARGS)
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "could not commit the `design` branch",
        ))
        .stderr(predicate::str::contains("user.email"));
    assert!(!tmp.path().join(".mod/socle.toml").exists());

    bin(tmp.path())
        .arg("init")
        .args(INIT_ARGS)
        .assert()
        .success();
    git(tmp.path(), &["rev-parse", "--verify", "design"]);
}

/// On an empty GitHub repository the first branch pushed becomes the
/// default: init pushes nothing, and leaves the publication to `mod setup`.
#[test]
fn init_leaves_the_remote_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("remote.git");
    let root = tmp.path().join("swift-blades");
    fs::create_dir_all(&root).unwrap();
    git(
        tmp.path(),
        &["init", "--quiet", "--bare", bare.to_str().unwrap()],
    );
    git(&root, &["init", "--quiet", "--initial-branch=26.3"]);
    let github = "https://github.com/payangar-studio/swift-blades.git";
    git(&root, &["remote", "add", "origin", github]);
    git(
        &root,
        &[
            "config",
            &format!("url.{}.insteadOf", bare.display()),
            github,
        ],
    );

    bin(&root)
        .arg("init")
        .args(INIT_ARGS)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "the 26.3 branch is pushed, run `mod setup`",
        ));

    git(&root, &["rev-parse", "--verify", "design"]);
    assert_eq!(git(&bare, &["for-each-ref"]), "");
}

#[test]
fn init_lays_down_the_design_skill() {
    let (_tmp, root) = init_repo();
    let text = fs::read_to_string(root.join(SKILL)).unwrap();
    let front = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front, _)| front)
        .expect("a frontmatter");
    assert!(front.contains("name: mod-design\n"));
    assert!(front.contains("Use when"));
    assert!(!text.contains('\u{2014}'), "no em dash in AI texts");
    let lock = SocleLock::load(&SOCLE, &root).unwrap();
    for path in [SKILL, CHECK] {
        assert!(
            lock.files.iter().any(|f| f.path == path),
            "{path} not owned"
        );
    }
}

/// Runs the skill's check on `html`; returns whether it passed and its output.
fn check(root: &Path, html: &str) -> (bool, String) {
    let page = root.join("page.html");
    fs::write(&page, html).unwrap();
    let output = Process::new("sh")
        .arg(root.join(CHECK))
        .arg(&page)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

#[test]
fn the_starter_page_is_self_contained() {
    let (_tmp, root) = init_repo();
    let page = git(&root, &["show", "design:docs/design/index.html"]);
    let (ok, output) = check(&root, &page);
    assert!(ok, "{output}");
}

#[test]
fn the_check_refuses_a_page_that_loads_anything() {
    let (_tmp, root) = init_repo();
    for (defect, html) in [
        (
            "stylesheet",
            "<link\n  rel=\"stylesheet\" href=\"https://cdn.example.com/x.css\">",
        ),
        ("script", "<script src=\"app.js\"></script>"),
        ("image file", "<img alt=\"\" src='textures/blade.png'>"),
        (
            "svg image",
            "<svg><image href=\"https://x.test/y.png\"/></svg>",
        ),
        ("video poster", "<video poster=cover.jpg></video>"),
        (
            "iframe",
            "<iframe src=\"https://www.youtube.com/embed/x\"></iframe>",
        ),
        (
            "css url",
            "<style>.a { background: url(\"texture.png\") }</style>",
        ),
        ("css import", "<style>@import \"fonts.css\";</style>"),
        ("fetch", "<script>fetch('/data.json')</script>"),
        (
            "module import",
            "<script type=\"module\">import { h } from \"https://esm.sh/preact\";</script>",
        ),
        ("claude runtime", "<script>window.claude.ask('x')</script>"),
    ] {
        let (ok, output) = check(
            &root,
            &format!("<!doctype html><html><body>{html}</body></html>"),
        );
        assert!(!ok, "{defect} passed: {output}");
        assert!(output.contains("not self-contained"), "{defect}: {output}");
    }
}

/// What looks like a resource but loads nothing.
#[test]
fn the_check_accepts_links_data_uris_and_fragments() {
    let (_tmp, root) = init_repo();
    let html = r##"<!doctype html><html><head><style>
  .a { background: url("data:image/png;base64,iVBORw0KGgo=") }
  .b { fill: url(#gradient) }
</style></head><body>
<a href="https://modrinth.com/mod/swift-blades">Modrinth</a>
<img alt="Blade"
  src="data:image/png;base64,iVBORw0KGgo=">
<svg><use href="#blade"/></svg>
<p>The blade can import (and export) items. data-src="x"</p>
<script>document.querySelector('a').focus();</script>
</body></html>"##;
    let (ok, output) = check(&root, html);
    assert!(ok, "{output}");
}
