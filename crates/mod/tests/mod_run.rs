//! `mod run --server` on a project made by `mod init`: the client joins the
//! dev server. It downloads Gradle, Fabric, Minecraft and a mod from
//! Modrinth, and runs the client on a virtual display (`xvfb-run`), so it is
//! `#[ignore]`: CI runs it in its own job (`cargo test -p mc-mod --test
//! mod_run -- --ignored`). It needs a JDK 25 and Xvfb.
#![cfg(unix)]

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command as Process, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin;

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

#[test]
#[ignore = "downloads Gradle, Fabric, Minecraft and Lithium; needs Xvfb"]
fn the_client_joins_the_dev_server() {
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
    fs::write(
        root.join("test-profiles/multi.toml"),
        "mods = [\"lithium\"]\n\n[options]\nrenderDistance = 4\n",
    )
    .unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "init"]);

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
