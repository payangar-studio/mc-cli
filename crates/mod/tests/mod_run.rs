//! `mod run` on a project made by `mod init`: with `--server`, the client
//! joins the dev server; a CI profile runs the Fabric gametests with its mods,
//! as the mod's workflow does. They download Gradle, Fabric, Minecraft and
//! mods from Modrinth, and run the client on a virtual display (`xvfb-run`),
//! so they are `#[ignore]`: CI runs them in their own job (`cargo test -p
//! mc-mod --test mod_run -- --ignored`). They need a JDK 25 and Xvfb.
#![cfg(unix)]

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin;
use yaml_rust2::{Yaml, YamlLoader};

/// The first build downloads Minecraft and compiles the build logic.
const TIMEOUT: Duration = Duration::from_secs(20 * 60);

fn git(root: &Path, args: &[&str]) -> String {
    let output = Process::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// A Fabric mod made by `mod init` in `<tmp>/mod`, with the test profiles
/// `profiles` (name, content), committed.
fn init(profiles: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("mod")
        .unwrap()
        .current_dir(tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path().join(".machine-config"))
        .args([
            "init",
            "mod",
            "--id=swift_blades",
            "--package=com.payangar.swiftblades",
            "--license=MIT",
            "--mc-version=26.3",
            "--name=Swift Blades",
            "--loaders=fabric",
            "--side=both-required",
        ])
        .assert()
        .success();
    let root = tmp.path().join("mod");
    fs::create_dir_all(root.join("test-profiles")).unwrap();
    for (name, text) in profiles {
        fs::write(root.join(format!("test-profiles/{name}.toml")), text).unwrap();
    }
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "init"]);
    (tmp, root)
}

#[test]
#[ignore = "downloads Gradle, Fabric, Minecraft and Lithium; needs Xvfb"]
fn the_client_joins_the_dev_server() {
    let (tmp, root) = init(&[(
        "multi",
        "mods = [\"lithium\"]\n\n[options]\nrenderDistance = 4\n",
    )]);

    let output = tmp.path().join("mod-run.log");
    let out = fs::File::create(&output).unwrap();
    let mut child = Process::new("xvfb-run")
        .args(["-a", "-s", "-screen 0 1280x720x24"])
        .arg(cargo_bin("mod"))
        .args(["run", "--profile", "multi", "--server"])
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        // Its own process group, stopped as a whole below.
        .process_group(0)
        .spawn()
        .unwrap();

    let log = root.join("runs/multi/server/logs/latest.log");
    let started = Instant::now();
    let joined = loop {
        let joined = fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .find(|l| l.contains("joined the game"))
            .map(str::to_owned);
        if let Some(line) = joined {
            break Some(line);
        }
        if child.try_wait().unwrap().is_some() || started.elapsed() > TIMEOUT {
            break None;
        }
        thread::sleep(Duration::from_secs(2));
    };
    let _ = Process::new("kill")
        .args(["-TERM", &format!("-{}", child.id())])
        .status();
    let _ = child.wait();

    let server_log = fs::read_to_string(&log).unwrap_or_default();
    eprintln!(
        "mod run output:\n{}\nserver log:\n{server_log}",
        fs::read_to_string(&output).unwrap_or_default()
    );
    let joined = joined.expect("the client never joined the dev server");
    eprintln!("joined: {joined}");
    assert!(
        root.join("runs/multi/server/mods")
            .read_dir()
            .unwrap()
            .count()
            == 1
    );
    assert!(
        fs::read_to_string(root.join("runs/multi/options.txt"))
            .unwrap()
            .contains("renderDistance:4")
    );
    assert_eq!(
        git(&root, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
}

/// Runs `script` as a `run` step of GitHub Actions does, in `root`.
fn step(root: &Path, script: &str, env: &[(String, String)]) -> (bool, String) {
    let output = Process::new("bash")
        .args(["--noprofile", "--norc", "-eo", "pipefail", "-c", script])
        .current_dir(root)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("$ {script}\n{log}");
    (output.status.success(), log)
}

/// The mod's workflow on a CI profile with Sodium: the `profiles` job lists
/// it, and the steps of its `profile` job, run here as the workflow writes
/// them with this build of `mod` in place of the release, pass the server and
/// client gametests with Sodium loaded and the screenshot comparison skipped.
#[test]
#[ignore = "downloads Gradle, Fabric, Minecraft and Sodium; needs Xvfb"]
fn a_ci_profile_runs_the_fabric_gametests_with_its_mods() {
    let (tmp, root) = init(&[
        ("sodium", "ci = true\nmods = [\"sodium\"]\n"),
        ("local", "mods = [\"lithium\"]\n"),
    ]);
    let text = fs::read_to_string(root.join(".github/workflows/mod-tests.yml")).unwrap();
    let workflow = &YamlLoader::load_from_str(&text).unwrap()[0];
    let jobs = &workflow["jobs"];

    let named = |job: &Yaml, name: &str| -> String {
        job["steps"]
            .as_vec()
            .unwrap()
            .iter()
            .find(|s| s["name"].as_str() == Some(name))
            .and_then(|s| s["run"].as_str())
            .unwrap_or_else(|| panic!("no step `{name}`"))
            .to_owned()
    };
    let output = tmp.path().join("github-output");
    let env = [("GITHUB_OUTPUT".to_owned(), output.display().to_string())];
    let (listed, _) = step(
        &root,
        &named(&jobs["profiles"], "List the CI profiles"),
        &env,
    );
    assert!(listed, "the CI profiles were not listed");
    assert_eq!(fs::read_to_string(&output).unwrap(), "names=[\"sodium\"]\n");

    // The job's environment, with the matrix value, and this build of `mod`.
    let job = &jobs["profile"];
    let mut env: Vec<(String, String)> = job["env"]
        .as_hash()
        .unwrap()
        .iter()
        .filter_map(|(k, v)| Some((k.as_str()?.to_owned(), v.as_str()?.to_owned())))
        .filter(|(_, v)| !v.contains("${{"))
        .collect();
    env.push(("PROFILE".into(), "sodium".into()));
    let bin = cargo_bin("mod");
    let path = std::env::var("PATH").unwrap_or_default();
    env.push((
        "PATH".into(),
        format!("{}:{path}", bin.parent().unwrap().display()),
    ));
    let mut gradle = String::new();
    for s in job["steps"].as_vec().unwrap() {
        let (Some(script), name) = (s["run"].as_str(), s["name"].as_str().unwrap_or("")) else {
            continue;
        };
        // CI has the packages, and `mod` is this build.
        if name.starts_with("Install") {
            continue;
        }
        let (ok, log) = step(&root, script, &env);
        assert!(ok, "step `{name}` failed");
        gradle.push_str(&log);
    }

    let mods: Vec<String> = fs::read_dir(root.join("runs/sodium/mods"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(mods.len() == 1 && mods[0].starts_with("sodium"), "{mods:?}");
    assert!(
        gradle.contains(":fabric:runGameTest ran 1 test(s)."),
        "the server gametest did not run"
    );
    let client_log =
        fs::read_to_string(root.join("fabric/build/run/clientGameTest/logs/latest.log")).unwrap();
    let sodium = client_log
        .lines()
        .find(|l| l.trim_start().starts_with("- sodium "))
        .expect("Sodium was not loaded in the client gametests");
    eprintln!("loaded: {}", sodium.trim());
    assert!(
        !root
            .join("fabric/build/run/clientGameTest/screenshots/example_world.png")
            .exists(),
        "the screenshot comparison ran under a test profile"
    );
    assert_eq!(
        git(&root, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
}
