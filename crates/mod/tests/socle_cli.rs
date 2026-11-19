//! Integration tests of `mod init` and `mod update`, in temporary directories.
//! The builds of the generated project are in `gradle_build.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use assert_cmd::Command;
use mc_mod::model::mod_toml::{Loader, ModToml, Side};
use mc_mod::model::socle::{LockedFile, LockedKind, SocleConfig, SocleLock};
use mc_mod::model::{sha256_bytes, sha256_file};
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

/// The binary without a terminal (assert_cmd pipes stdin and stdout), with a
/// machine configuration of its own.
fn bin(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("mod").unwrap();
    cmd.current_dir(dir)
        .env_remove("RUST_LOG")
        .env("XDG_CONFIG_HOME", dir.join(".machine-config"));
    cmd
}

/// A fresh repository in `<tmp>/swift-blades`.
fn init_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    bin(tmp.path())
        .arg("init")
        .arg("swift-blades")
        .args(INIT_ARGS)
        .assert()
        .success()
        .stdout(predicate::str::contains("Created mod `Swift Blades`"));
    let root = tmp.path().join("swift-blades");
    (tmp, root)
}

/// Pretends the repository was written by an older CLI, so that `update` runs.
fn make_older(root: &Path) {
    let mut config = SocleConfig::load(&SOCLE, root).unwrap();
    config.version = "0.0.1".into();
    config.save(&SOCLE, root).unwrap();
}

#[test]
fn init_creates_a_complete_repository() {
    let (_tmp, root) = init_repo();
    for path in [
        "mod.toml",
        ".mod/socle.toml",
        ".mod/socle.lock",
        ".gitignore",
        "settings.gradle.kts",
        "gradlew",
        "gradle/wrapper/gradle-wrapper.jar",
        "build-logic/build.gradle.kts",
        "common/build.gradle.kts",
        "fabric/build.gradle.kts",
        "neoforge/build.gradle.kts",
        "common/src/main/java/com/payangar/swiftblades/CommonEntrypoint.java",
        "common/src/client/java/com/payangar/swiftblades/client/ClientEntrypoint.java",
        "page/page.toml",
        "changelogs/README.md",
    ] {
        assert!(root.join(path).is_file(), "{path} missing");
    }

    let module = ModToml::load(&root).unwrap();
    assert_eq!(module.id, "swift_blades");
    assert_eq!(module.side, Side::BothRequired);
    assert_eq!(module.loaders, [Loader::Fabric, Loader::Neoforge]);
    assert_eq!(module.dependencies[0].id, "fabric-api");

    let config = SocleConfig::load(&SOCLE, &root).unwrap();
    assert_eq!(config.version, SOCLE.cli_version().to_string());
    assert_eq!(config.init.minecraft, "26.3");
    assert!(config.init.fabric_api.ends_with("+26.3"));

    let lock = SocleLock::load(&SOCLE, &root).unwrap();
    assert_eq!(
        lock.files.len(),
        SOCLE.socle_files().len() + SOCLE.socle_blocks().len()
    );
    assert!(SOCLE.edited_by_hand(&root, &lock).unwrap().is_empty());
    // The mod's own files are not the socle's.
    assert!(lock.files.iter().all(|f| f.path != "mod.toml"));

    let branch = Process::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "26.3");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(root.join("gradlew"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "gradlew is not executable");
    }
}

#[test]
fn init_takes_the_package_from_the_machine_configuration() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join(".machine-config/minecraft");
    fs::create_dir_all(&config).unwrap();
    fs::write(
        config.join("config.toml"),
        "[identity]\npackage = \"com.payangar\"\n",
    )
    .unwrap();
    let args = INIT_ARGS.iter().filter(|a| !a.starts_with("--package"));
    bin(tmp.path()).arg("init").args(args).assert().success();
    let module = ModToml::load(tmp.path()).unwrap();
    assert_eq!(module.package, "com.payangar.swiftblades");
}

#[test]
fn init_without_a_terminal_names_the_missing_argument() {
    let tmp = tempfile::tempdir().unwrap();
    for missing in [
        "--id",
        "--name",
        "--package",
        "--side",
        "--loaders",
        "--license",
        "--mc-version",
    ] {
        let args = INIT_ARGS.iter().filter(|a| !a.starts_with(missing));
        bin(tmp.path())
            .arg("init")
            .args(args)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(format!("pass {missing}")));
    }
    assert!(!tmp.path().join(".mod").exists());
}

#[test]
fn init_refuses_bad_values() {
    for (bad, message) in [
        ("--id=Swift", "not a valid mod id"),
        ("--package=Com.Payangar", "not a valid Java package"),
        ("--side=everywhere", "unknown side"),
        ("--loaders=forge", "unknown loader"),
        ("--mc-version=1.21.1", "not supported"),
        ("--name=", "pass --name"),
        ("--license= ", "pass --license"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let name = bad.split('=').next().unwrap();
        let args = INIT_ARGS.iter().filter(|a| !a.starts_with(name));
        bin(tmp.path())
            .arg("init")
            .args(args)
            .arg(bad)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(message));
        assert!(!tmp.path().join(".mod").exists(), "{bad}");
    }
}

#[test]
fn init_refuses_an_existing_socle() {
    let (_tmp, root) = init_repo();
    bin(&root)
        .arg("init")
        .args(INIT_ARGS)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already holds a mod socle"));
}

/// An init that stopped before writing the socle settings left `.mod/` without
/// its marker: the directory is not a mod repository yet, and init resumes.
#[test]
fn init_resumes_after_an_interrupted_init() {
    let (_tmp, root) = init_repo();
    fs::remove_file(root.join(".mod/socle.toml")).unwrap();
    bin(&root).arg("init").args(INIT_ARGS).assert().success();
    assert!(root.join(".mod/socle.toml").is_file());
}

#[test]
fn init_refuses_to_overwrite_existing_build_files_unless_forced() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("swift-blades");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("gradle.properties"), "mod_version=1.4.2\n").unwrap();
    fs::write(root.join("settings.gradle.kts"), "// mine\n").unwrap();
    let mine = |root: &Path| {
        (
            fs::read_to_string(root.join("gradle.properties")).unwrap(),
            fs::read_to_string(root.join("settings.gradle.kts")).unwrap(),
        )
    };
    let before = mine(&root);

    bin(tmp.path())
        .arg("init")
        .arg("swift-blades")
        .args(INIT_ARGS)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("gradle.properties"))
        .stderr(predicate::str::contains("settings.gradle.kts"))
        .stderr(predicate::str::contains("--force"));
    assert_eq!(mine(&root), before);
    assert!(!root.join(".mod").exists());
    assert!(!root.join("mod.toml").exists());

    bin(tmp.path())
        .arg("init")
        .arg("swift-blades")
        .args(INIT_ARGS)
        .arg("--force")
        .assert()
        .success();
    let lock = SocleLock::load(&SOCLE, &root).unwrap();
    assert!(SOCLE.edited_by_hand(&root, &lock).unwrap().is_empty());
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Process::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[test]
fn git_ignores_the_dev_runs_but_not_a_run_package() {
    let (_tmp, root) = init_repo();
    let ignored = |path: &str| {
        Process::new("git")
            .args(["check-ignore", "--quiet", "--no-index", path])
            .current_dir(&root)
            .status()
            .unwrap()
            .success()
    };
    for path in [
        "fabric/run/options.txt",
        "neoforge/run/logs/latest.log",
        "runs/sodium/options.txt",
    ] {
        assert!(ignored(path), "{path} is not ignored");
    }
    assert!(!ignored("common/src/main/java/com/x/run/Run.java"));
}

#[test]
fn init_names_the_branch_of_an_empty_repository() {
    let tmp = tempfile::tempdir().unwrap();
    git(tmp.path(), &["init", "--quiet", "--initial-branch=main"]);
    bin(tmp.path())
        .arg("init")
        .args(INIT_ARGS)
        .assert()
        .success();
    assert_eq!(git(tmp.path(), &["branch", "--show-current"]), "26.3");
}

#[test]
fn init_warns_when_the_branch_is_not_the_minecraft_version() {
    let tmp = tempfile::tempdir().unwrap();
    git(tmp.path(), &["init", "--quiet", "--initial-branch=main"]);
    git(
        tmp.path(),
        &["commit", "--quiet", "--allow-empty", "-m", "start"],
    );
    bin(tmp.path())
        .arg("init")
        .args(INIT_ARGS)
        .assert()
        .success()
        .stderr(predicate::str::contains("`main`"))
        .stderr(predicate::str::contains("26.3"));
    assert_eq!(git(tmp.path(), &["branch", "--show-current"]), "main");
}

#[test]
fn update_is_a_no_op_when_versions_match() {
    let (_tmp, root) = init_repo();
    let lock = fs::read_to_string(root.join(".mod/socle.lock")).unwrap();
    bin(&root)
        .arg("update")
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to update"));
    assert_eq!(
        fs::read_to_string(root.join(".mod/socle.lock")).unwrap(),
        lock
    );
}

#[test]
fn update_refuses_a_hand_edited_file_and_force_restores_it() {
    let (_tmp, root) = init_repo();
    let path = root.join("build-logic/build.gradle.kts");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, "// mine\n").unwrap();
    make_older(&root);

    bin(&root)
        .arg("update")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("edited by hand"))
        .stderr(predicate::str::contains("build-logic/build.gradle.kts"))
        .stderr(predicate::str::contains("mod update --force"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "// mine\n");

    bin(&root).args(["update", "--force"]).assert().success();
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    let config = SocleConfig::load(&SOCLE, &root).unwrap();
    assert_eq!(config.version, SOCLE.cli_version().to_string());
    assert_eq!(
        config.init.minecraft, "26.3",
        "update keeps the pinned versions"
    );
}

#[test]
fn update_removes_files_that_left_the_socle() {
    let (_tmp, root) = init_repo();
    let stale = "build-logic/src/main/kotlin/Gone.kt";
    fs::write(root.join(stale), "// old\n").unwrap();
    let mut lock = SocleLock::load(&SOCLE, &root).unwrap();
    lock.files.push(LockedFile {
        path: stale.into(),
        sha256: sha256_bytes(b"// old\n"),
        kind: LockedKind::File,
    });
    lock.save(&SOCLE, &root).unwrap();
    make_older(&root);

    bin(&root)
        .arg("update")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("Removed {stale}")));
    assert!(!root.join(stale).exists());
    let lock = SocleLock::load(&SOCLE, &root).unwrap();
    assert!(lock.files.iter().all(|f| f.path != stale));
    for entry in lock.files.iter().filter(|f| f.kind == LockedKind::File) {
        assert_eq!(sha256_file(&root.join(&entry.path)).unwrap(), entry.sha256);
    }
}

#[test]
fn update_keeps_the_mods_own_files() {
    let (_tmp, root) = init_repo();
    let entrypoint =
        root.join("common/src/main/java/com/payangar/swiftblades/CommonEntrypoint.java");
    fs::write(&entrypoint, "// mine\n").unwrap();
    fs::write(root.join("mod.toml"), "# mine\n").unwrap();
    make_older(&root);
    bin(&root).arg("update").assert().success();
    assert_eq!(fs::read_to_string(&entrypoint).unwrap(), "// mine\n");
    assert_eq!(
        fs::read_to_string(root.join("mod.toml")).unwrap(),
        "# mine\n"
    );
}
