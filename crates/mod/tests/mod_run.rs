//! `mod run` on a project made by `mod init`: with `--server`, the client
//! joins the dev server, and closing it stops the server; a CI profile runs the Fabric gametests with its mods,
//! as the mod's workflow does; the Fabric client loads Mod Menu when the mod
//! has a client side. They download Gradle, Fabric, Minecraft and
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

/// A Fabric mod made by `mod init` in `<tmp>/mod`, on `side`, with the test
/// profiles `profiles` (name, content), committed.
fn init(side: &str, profiles: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
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
            &format!("--side={side}"),
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

/// The `java` processes whose working directory is `dir`.
fn java_in(dir: &Path) -> Vec<String> {
    let dir = dir.canonicalize().unwrap();
    fs::read_dir("/proc")
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            fs::read_link(path.join("cwd")).is_ok_and(|cwd| cwd == dir)
                && fs::read(path.join("cmdline"))
                    .is_ok_and(|c| String::from_utf8_lossy(&c).contains("java"))
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

/// The profile's options survive the game's start: `preferredGraphicsBackend`
/// is one the game resets in an options.txt without a data version. Closing
/// the client then stops the server through RCON, and `mod run` with it.
#[test]
#[ignore = "downloads Gradle, Fabric, Minecraft and Lithium; needs Xvfb"]
fn the_client_joins_the_dev_server() {
    let (tmp, root) = init(
        "both-required",
        &[(
            "multi",
            "mods = [\"lithium@mc26.3-0.26.3-fabric\"]\n\n[options]\nrenderDistance = 4\n\
         preferredGraphicsBackend = '\"opengl\"'\n",
        )],
    );

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
    let multi = root.join("runs/multi");
    // Closing the client: `mod run` stops the server and exits by itself.
    let closed = joined.is_some() && {
        let clients = java_in(&multi);
        eprintln!("closing the client: {clients:?}");
        for pid in &clients {
            let _ = Process::new("kill").args(["-TERM", pid]).status();
        }
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if child.try_wait().unwrap().is_some() {
                break true;
            }
            if Instant::now() > deadline {
                break false;
            }
            thread::sleep(Duration::from_secs(1));
        }
    };
    eprintln!(
        "stopping mod run after {}s (joined: {}, stopped with the client: {closed})",
        started.elapsed().as_secs(),
        joined.is_some()
    );
    stop(&mut child);
    keep_logs(
        "server",
        &[
            (output.clone(), "mod-run.log"),
            (multi.join("logs"), "client-logs"),
            (multi.join("crash-reports"), "client-crash-reports"),
            (multi.join("server/logs"), "server-logs"),
            (multi.join("server/gradle.log"), "server-gradle.log"),
        ],
    );

    let server_log = fs::read_to_string(&log).unwrap_or_default();
    eprintln!(
        "mod run output:\n{}\nserver log:\n{server_log}",
        fs::read_to_string(&output).unwrap_or_default()
    );
    let joined = joined.expect("the client never joined the dev server");
    eprintln!("joined: {joined}");
    assert!(closed, "mod run did not stop when the client closed");
    assert!(
        server_log.contains("RCON Client") && server_log.contains("Stopping server"),
        "the server was not stopped through RCON"
    );
    let options = fs::read_to_string(multi.join("options.txt")).unwrap();
    assert!(
        options
            .lines()
            .any(|l| l == "preferredGraphicsBackend:\"opengl\""),
        "the profile's option was reset:\n{options}"
    );
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

/// Runs `script` as a `run` step of GitHub Actions does, in `root`, for at
/// most [`TIMEOUT`]; returns whether it passed and its output, also kept in
/// `log`.
fn step(root: &Path, script: &str, env: &[(String, String)], log: &Path) -> (bool, String) {
    eprintln!("$ {script}");
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let out = fs::File::create(log).unwrap();
    let mut child = Process::new("bash")
        .args(["--noprofile", "--norc", "-eo", "pipefail", "-c", script])
        .current_dir(root)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        // Its own process group, stopped as a whole on timeout.
        .process_group(0)
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if started.elapsed() > TIMEOUT {
            stop(&mut child);
            break None;
        }
        thread::sleep(Duration::from_secs(2));
    };
    let output = fs::read_to_string(log).unwrap_or_default();
    eprintln!("{output}");
    if status.is_none() {
        eprintln!("timed out after {} minutes", TIMEOUT.as_secs() / 60);
    }
    (status.is_some_and(|s| s.success()), output)
}

/// Stops the process group `child` leads: TERM, then KILL when it is still
/// there after 30 seconds.
fn stop(child: &mut std::process::Child) {
    let group = format!("-{}", child.id());
    for signal in ["-TERM", "-KILL"] {
        let kill = Process::new("kill").args([signal, "--", &group]).output();
        eprintln!("kill {signal} -- {group}: {kill:?}");
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                return;
            }
            thread::sleep(Duration::from_secs(1));
        }
        let ps = Process::new("ps")
            .args(["-eo", "pid,ppid,pgid,stat,args"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
        eprintln!("still running after {signal}:\n{}", ps.unwrap_or_default());
    }
}

/// Copies each `(file or directory, name)` that exists to
/// `$MOD_TESTS_ARTIFACTS/<test>/<name>`, when set: CI uploads them.
fn keep_logs(test: &str, kept: &[(PathBuf, &str)]) {
    let Some(dir) = std::env::var_os("MOD_TESTS_ARTIFACTS") else {
        return;
    };
    let to = Path::new(&dir).join(test);
    fs::create_dir_all(&to).unwrap();
    for (from, name) in kept {
        if from.is_dir() {
            copy_dir(from, &to.join(name));
        } else if from.is_file() {
            fs::copy(from, to.join(name)).unwrap();
        }
    }
}

/// The step outputs under `steps` and the logs of the Fabric game runs of
/// `root`, for [`keep_logs`].
fn fabric_run_logs(steps: &Path, root: &Path) -> Vec<(PathBuf, String)> {
    let mut kept = vec![(steps.to_owned(), "steps".to_owned())];
    if let Ok(runs) = fs::read_dir(root.join("fabric/build/run")) {
        for run in runs.map(|e| e.unwrap().path()) {
            let run_name = run.file_name().unwrap().to_string_lossy().into_owned();
            for dir in ["logs", "crash-reports"] {
                kept.push((run.join(dir), format!("{run_name}-{dir}")));
            }
        }
    }
    kept
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().map(|e| e.unwrap()) {
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

/// The mod's workflow on a CI profile with Sodium: the `profiles` job lists
/// it, and the steps of its `profile` job, run here as the workflow writes
/// them with this build of `mod` in place of the release, pass the server and
/// client gametests with Sodium loaded and the screenshot comparison skipped.
#[test]
#[ignore = "downloads Gradle, Fabric, Minecraft and Sodium; needs Xvfb"]
fn a_ci_profile_runs_the_fabric_gametests_with_its_mods() {
    let (tmp, root) = init(
        "both-required",
        &[
            (
                "sodium",
                "ci = true\nmods = [\"sodium@mc26.3-0.9.2-fabric\"]\n",
            ),
            ("local", "mods = [\"lithium@mc26.3-0.26.3-fabric\"]\n"),
        ],
    );
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
    let steps = tmp.path().join("steps");
    let output = tmp.path().join("github-output");
    let env = [("GITHUB_OUTPUT".to_owned(), output.display().to_string())];
    let (listed, _) = step(
        &root,
        &named(&jobs["profiles"], "List the CI profiles"),
        &env,
        &steps.join("0-list.log"),
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
    for (i, s) in job["steps"].as_vec().unwrap().iter().enumerate() {
        let (Some(script), name) = (s["run"].as_str(), s["name"].as_str().unwrap_or("")) else {
            continue;
        };
        // CI has the packages, and `mod` is this build.
        if name.starts_with("Install") {
            continue;
        }
        let (ok, log) = step(&root, script, &env, &steps.join(format!("{}.log", i + 1)));
        let kept = fabric_run_logs(&steps, &root);
        let kept: Vec<_> = kept.iter().map(|(p, n)| (p.clone(), n.as_str())).collect();
        keep_logs("ci_profile", &kept);
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

/// The mods the Fabric client of a plain `mod run` loads, for a mod on
/// `side`: their lines of the loader's list (`- modmenu 21.0.1`), and the
/// Mod Menu version `.mod/socle.toml` pins. The client is stopped once it has
/// logged them.
fn client_mods(side: &str) -> (Vec<String>, String) {
    let (tmp, root) = init(side, &[]);
    let output = tmp.path().join("mod-run.log");
    let out = fs::File::create(&output).unwrap();
    let mut child = Process::new("xvfb-run")
        .args(["-a", "-s", "-screen 0 1280x720x24"])
        .arg(cargo_bin("mod"))
        .arg("run")
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        // Its own process group, stopped as a whole below.
        .process_group(0)
        .spawn()
        .unwrap();

    let dir = root.join("runs/.one-shot");
    let log = dir.join("logs/latest.log");
    let started = Instant::now();
    let listed = |text: &str| {
        text.lines()
            .any(|l| l.contains("Loading ") && l.contains(" mods:"))
    };
    let mut listed_at: Option<Instant> = None;
    let text = loop {
        let text = fs::read_to_string(&log).unwrap_or_default();
        if listed_at.is_none() && listed(&text) {
            listed_at = Some(Instant::now());
        }
        // The list is one log event: a few seconds more and it is written.
        if listed_at.is_some_and(|at| at.elapsed() > Duration::from_secs(5))
            || child.try_wait().unwrap().is_some()
            || started.elapsed() > TIMEOUT
        {
            break text;
        }
        thread::sleep(Duration::from_secs(1));
    };
    for pid in java_in(&dir) {
        let _ = Process::new("kill").args(["-TERM", &pid]).status();
    }
    stop(&mut child);
    keep_logs(
        &format!("modmenu-{side}"),
        &[
            (output.clone(), "mod-run.log"),
            (dir.join("logs"), "client-logs"),
            (dir.join("crash-reports"), "client-crash-reports"),
        ],
    );
    assert!(
        listed(&text),
        "the client never listed its mods; mod run output:\n{}\nclient log:\n{text}",
        fs::read_to_string(&output).unwrap_or_default()
    );
    let mods: Vec<String> = text
        .lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("- "))
        .map(str::to_owned)
        .collect();
    eprintln!("loaded on the client:\n{}", mods.join("\n"));
    let socle: toml::Table = fs::read_to_string(root.join(".mod/socle.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let modmenu = socle["init"]["modmenu"].as_str().unwrap().to_owned();
    (mods, modmenu)
}

#[test]
#[ignore = "downloads Gradle, Fabric, Minecraft and Mod Menu; needs Xvfb"]
fn the_fabric_client_of_a_mod_with_a_client_side_loads_mod_menu() {
    let (mods, modmenu) = client_mods("both-required");
    assert!(
        mods.iter().any(|l| *l == format!("- modmenu {modmenu}")),
        "Mod Menu {modmenu}, pinned in .mod/socle.toml, was not loaded"
    );
}

#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft; needs Xvfb"]
fn the_fabric_client_of_a_server_only_mod_does_without_mod_menu() {
    let (mods, _) = client_mods("server");
    assert!(
        mods.iter().any(|l| l.starts_with("- minecraft ")),
        "the mod list was not read"
    );
    assert!(
        !mods.iter().any(|l| l.starts_with("- modmenu ")),
        "Mod Menu was loaded for a server-only mod"
    );
}
