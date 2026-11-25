//! `mod run` against a mocked Modrinth API, with a fake Gradle wrapper in
//! place of the game.

use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::process::Command;

use mc_core::hash::copy_hashing;
use mc_core::model::entry::Pack;
use mc_core::platform::Registry;
use mc_core::platform::modrinth::Modrinth;
use mockito::{Matcher, Mock, Server, ServerGuard};
use serde_json::json;

use super::*;
use crate::model::test_profile::Item;

#[path = "../../../modpack/tests/common/mod.rs"]
mod common;

fn pack() -> Pack {
    Pack {
        minecraft: "26.3".into(),
        loader: "fabric".into(),
        loader_version: "0.20.1".into(),
    }
}

/// A Modrinth API with three projects: `sodium` (client only, two versions),
/// `lithium` (both sides) and `faithful` (a resource pack).
struct Api {
    server: ServerGuard,
    mocks: Vec<Mock>,
}

impl Api {
    fn new() -> Self {
        let mut api = Self {
            server: Server::new(),
            mocks: Vec::new(),
        };
        api.project(
            "sodium",
            "mod",
            "required",
            "unsupported",
            &["0.9.3", "0.9.2"],
        );
        api.project("lithium", "mod", "required", "required", &["0.26.3"]);
        api.project(
            "faithful",
            "resourcepack",
            "required",
            "unsupported",
            &["1.0"],
        );
        api
    }

    fn registry(&self) -> Registry {
        Registry::new(vec![Box::new(Modrinth::new(&self.server.url()))])
    }

    fn bytes(slug: &str, version: &str) -> Vec<u8> {
        format!("{slug} {version}").into_bytes()
    }

    fn file_name(slug: &str, version: &str) -> String {
        let ext = if slug == "faithful" { "zip" } else { "jar" };
        format!("{slug}-{version}.{ext}")
    }

    fn project(&mut self, slug: &str, kind: &str, client: &str, server: &str, versions: &[&str]) {
        let id = format!("id-{slug}");
        let url = self.server.url();
        let version = |v: &str| {
            let bytes = Self::bytes(slug, v);
            let hashes = copy_hashing(&mut Cursor::new(&bytes), &mut std::io::sink()).unwrap();
            json!({
                "id": format!("v-{slug}-{v}"),
                "project_id": id,
                "version_number": v,
                "version_type": "release",
                "files": [{
                    "hashes": { "sha1": hashes.sha1, "sha512": hashes.sha512 },
                    "url": format!("{url}/files/{}", Self::file_name(slug, v)),
                    "filename": Self::file_name(slug, v),
                    "primary": true,
                    "size": bytes.len(),
                }],
                "dependencies": [],
            })
        };
        let project = json!({
            "id": id, "slug": slug, "project_type": kind, "title": slug,
            "client_side": client, "server_side": server,
        });
        for path in [format!("/project/{slug}"), format!("/project/{id}")] {
            let mock = self
                .server
                .mock("GET", path.as_str())
                .with_body(project.to_string())
                .create();
            self.mocks.push(mock);
        }
        let all: Vec<_> = versions.iter().map(|v| version(v)).collect();
        let mock = self
            .server
            .mock("GET", format!("/project/{id}/version").as_str())
            .match_query(Matcher::Any)
            .with_body(serde_json::Value::Array(all).to_string())
            .create();
        self.mocks.push(mock);
        for v in versions {
            let mock = self
                .server
                .mock("GET", format!("/version/v-{slug}-{v}").as_str())
                .with_body(version(v).to_string())
                .create();
            self.mocks.push(mock);
        }
    }

    /// Serves the file of `slug` at `version`, expecting `hits` downloads.
    fn file(&mut self, slug: &str, version: &str, hits: usize) -> Mock {
        self.server
            .mock(
                "GET",
                format!("/files/{}", Self::file_name(slug, version)).as_str(),
            )
            .with_body(Self::bytes(slug, version))
            .expect(hits)
            .create()
    }
}

fn wanted(list: &[&str]) -> Vec<Item> {
    list.iter().map(|w| Item::parse(w).unwrap()).collect()
}

fn plan(dir: &Path, mods: &[&str], packs: &[&str], server: bool) -> Plan {
    Plan {
        dir: dir.to_path_buf(),
        loader: Loader::Fabric,
        mods: wanted(mods),
        packs: wanted(packs),
        local: dir.join("local"),
        options: vec![("renderDistance".into(), "8".into())],
        server,
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .map(|d| {
            d.map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn prepare_downloads_each_side_its_mods_and_enables_the_packs() {
    let mut api = Api::new();
    let files = [
        api.file("sodium", "0.9.3", 1),
        api.file("lithium", "0.26.3", 1),
        api.file("faithful", "1.0", 1),
    ];
    let dir = tempfile::tempdir().unwrap();
    let plan = plan(dir.path(), &["sodium", "lithium"], &["faithful"], true);

    let ports = prepare(&plan, &pack(), &api.registry()).unwrap().unwrap();

    assert_eq!(
        names(&dir.path().join("mods")),
        ["lithium-0.26.3.jar", "sodium-0.9.3.jar"]
    );
    assert_eq!(
        fs::read(dir.path().join("mods/sodium-0.9.3.jar")).unwrap(),
        b"sodium 0.9.3"
    );
    assert_eq!(
        names(&dir.path().join("resourcepacks")),
        ["faithful-1.0.zip"]
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("options.txt")).unwrap(),
        "onboardAccessibility:false\nrenderDistance:8\nresourcePacks:[\"vanilla\",\"file/faithful-1.0.zip\"]\n"
    );
    // Sodium is client only: the server takes Lithium alone, linked from the
    // client's copy rather than downloaded again.
    assert_eq!(
        names(&dir.path().join("server/mods")),
        ["lithium-0.26.3.jar"]
    );
    let properties = fs::read_to_string(dir.path().join("server/server.properties")).unwrap();
    assert!(
        properties.contains(&format!("server-port={}\n", ports.game)),
        "{properties}"
    );
    assert!(properties.contains("online-mode=false\n"), "{properties}");
    for file in files {
        file.assert();
    }
}

#[test]
fn a_pinned_version_is_taken_and_kept_between_runs() {
    let mut api = Api::new();
    let old = api.file("sodium", "0.9.2", 1);
    let dir = tempfile::tempdir().unwrap();
    let plan = plan(dir.path(), &["sodium@0.9.2"], &[], false);
    let registry = api.registry();

    assert_eq!(prepare(&plan, &pack(), &registry).unwrap(), None);
    // Already there with the same content: not downloaded again.
    prepare(&plan, &pack(), &registry).unwrap();
    assert_eq!(names(&dir.path().join("mods")), ["sodium-0.9.2.jar"]);
    old.assert();

    // By its Modrinth version id too.
    let plan = super::Plan {
        mods: wanted(&["sodium@v-sodium-0.9.2"]),
        ..plan
    };
    prepare(&plan, &pack(), &registry).unwrap();
    old.assert();
}

#[test]
fn what_is_no_longer_asked_is_removed() {
    let mut api = Api::new();
    let _files = [
        api.file("sodium", "0.9.3", 1),
        api.file("faithful", "1.0", 1),
    ];
    let dir = tempfile::tempdir().unwrap();
    let registry = api.registry();
    prepare(
        &plan(dir.path(), &["sodium"], &["faithful"], false),
        &pack(),
        &registry,
    )
    .unwrap();
    fs::write(dir.path().join("mods/stray.jar"), b"x").unwrap();

    prepare(&plan(dir.path(), &[], &[], false), &pack(), &registry).unwrap();

    assert!(names(&dir.path().join("mods")).is_empty());
    assert!(names(&dir.path().join("resourcepacks")).is_empty());
    assert_eq!(
        fs::read_to_string(dir.path().join("options.txt")).unwrap(),
        "onboardAccessibility:false\nrenderDistance:8\nresourcePacks:[\"vanilla\"]\n"
    );
}

/// A GitHub release's file of the loader goes to both sides, as any jar no
/// platform describes, and the next run downloads nothing.
#[test]
fn github_files_are_downloaded_once_and_placed_on_both_sides() {
    let mut api = Api::new();
    let url = api.server.url();
    let assets: Vec<_> = ["touch-fabric-1.0.jar", "touch-neoforge-1.0.jar"]
        .map(|name| {
            json!({
                "name": name, "size": name.len(),
                "browser_download_url": format!("{url}/gh/{name}"),
            })
        })
        .into();
    let release = json!({
        "tag_name": "v1", "draft": false, "prerelease": false, "assets": assets,
    });
    let _release = api
        .server
        .mock("GET", "/repos/o/touch/releases/tags/v1")
        .with_body(release.to_string())
        .create();
    let file = api
        .server
        .mock("GET", "/gh/touch-fabric-1.0.jar")
        .with_body("touch-fabric-1.0.jar")
        .expect(1)
        .create();
    let cache = tempfile::tempdir().unwrap();
    let platforms: Vec<Box<dyn mc_core::platform::Platform>> = vec![
        Box::new(Modrinth::new(&url)),
        Box::new(mc_core::platform::github::GitHub::new(
            &url,
            cache.path().to_path_buf(),
        )),
    ];
    let registry = Registry::new(platforms);
    let dir = tempfile::tempdir().unwrap();
    let plan = plan(dir.path(), &["github:o/touch@v1"], &[], true);

    for _ in 0..2 {
        prepare(&plan, &pack(), &registry).unwrap();
        for mods in ["mods", "server/mods"] {
            assert_eq!(names(&dir.path().join(mods)), ["touch-fabric-1.0.jar"]);
        }
    }
    file.assert();
}

/// A file of the profiles' local directory is copied to both sides, in the
/// packs' order, kept by the next run, replaced when it changes, and removed
/// once no longer asked.
#[test]
fn local_files_are_placed_like_the_others() {
    let mut api = Api::new();
    let _files = [
        api.file("lithium", "0.26.3", 1),
        api.file("faithful", "1.0", 1),
    ];
    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("local");
    fs::create_dir_all(&local).unwrap();
    fs::write(local.join("touch.jar"), b"touch").unwrap();
    fs::write(local.join("ours.zip"), b"ours").unwrap();
    let registry = api.registry();
    let local_plan = plan(
        dir.path(),
        &["local/touch.jar", "lithium"],
        &["local/ours.zip", "faithful"],
        true,
    );

    for _ in 0..2 {
        prepare(&local_plan, &pack(), &registry).unwrap();
        for mods in ["mods", "server/mods"] {
            assert_eq!(
                names(&dir.path().join(mods)),
                ["lithium-0.26.3.jar", "touch.jar"]
            );
        }
        assert_eq!(
            names(&dir.path().join("resourcepacks")),
            ["faithful-1.0.zip", "ours.zip"]
        );
        let options = fs::read_to_string(dir.path().join("options.txt")).unwrap();
        assert!(
            options.contains(
                "resourcePacks:[\"vanilla\",\"file/ours.zip\",\"file/faithful-1.0.zip\"]"
            ),
            "{options}"
        );
    }

    fs::write(local.join("touch.jar"), b"touch 2").unwrap();
    prepare(&local_plan, &pack(), &registry).unwrap();
    for mods in ["mods", "server/mods"] {
        assert_eq!(
            fs::read(dir.path().join(mods).join("touch.jar")).unwrap(),
            b"touch 2"
        );
    }

    prepare(
        &plan(dir.path(), &["lithium"], &["faithful"], true),
        &pack(),
        &registry,
    )
    .unwrap();
    for mods in ["mods", "server/mods"] {
        assert_eq!(names(&dir.path().join(mods)), ["lithium-0.26.3.jar"]);
    }
    assert_eq!(
        names(&dir.path().join("resourcepacks")),
        ["faithful-1.0.zip"]
    );
    assert_eq!(names(&local), ["ours.zip", "touch.jar"]);
}

/// Two files of one name would overwrite each other in the run directory.
#[test]
fn a_local_file_cannot_share_a_file_name() {
    let api = Api::new();
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("local")).unwrap();
    fs::write(dir.path().join("local/lithium-0.26.3.jar"), b"x").unwrap();
    let err = prepare(
        &plan(
            dir.path(),
            &["lithium", "local/lithium-0.26.3.jar"],
            &[],
            false,
        ),
        &pack(),
        &api.registry(),
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("two mods of the run have the file name `lithium-0.26.3.jar`"),
        "{err}"
    );
    assert_eq!(err.kind(), mc_core::error::ErrorKind::Usage);
}

#[test]
fn an_unknown_version_lists_the_available_ones() {
    let api = Api::new();
    let dir = tempfile::tempdir().unwrap();
    let err = prepare(
        &plan(dir.path(), &["sodium@9"], &[], false),
        &pack(),
        &api.registry(),
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("`sodium` has no version `9` for Minecraft 26.3 with fabric"),
        "{err}"
    );
    assert_eq!(err.hint(), "pick one of: 0.9.3, 0.9.2");
}

#[test]
fn a_project_of_the_other_kind_is_refused() {
    let api = Api::new();
    let dir = tempfile::tempdir().unwrap();
    let err = prepare(
        &plan(dir.path(), &["faithful"], &[], false),
        &pack(),
        &api.registry(),
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("`faithful` is a resource pack, not a mod"),
        "{err}"
    );
    let err = prepare(
        &plan(dir.path(), &[], &["lithium"], false),
        &pack(),
        &api.registry(),
    )
    .unwrap_err();
    assert!(err.hint().contains("--with"), "{}", err.hint());
}

#[test]
fn an_unknown_project_is_a_usage_error() {
    let mut api = Api::new();
    let _missing = api
        .server
        .mock("GET", "/project/nope")
        .with_status(404)
        .create();
    let dir = tempfile::tempdir().unwrap();
    let err = prepare(
        &plan(dir.path(), &["nope"], &[], false),
        &pack(),
        &api.registry(),
    )
    .unwrap_err();
    assert_eq!(err.kind(), mc_core::error::ErrorKind::Usage);
}

/// A repository with the socle's `.gitignore` block, committed, and a fake
/// Gradle wrapper. The wrapper records its arguments in `<root>/../calls`
/// and plays the game: the server writes its `Done` line and waits, the
/// client writes its log and exits.
fn repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("mod");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join(".gitignore"),
        include_str!("../../templates/blocks/gitignore"),
    )
    .unwrap();
    let calls = tmp.path().join("calls");
    common::write_executable(
        &root.join("gradlew"),
        &format!(
            r#"#!/bin/sh
echo "$@" >> '{calls}'
for arg in "$@"; do
  case "$arg" in -Pmod.runDir=*) dir="${{arg#-Pmod.runDir=}}" ;; esac
done
mkdir -p "$dir/logs"
case "$1" in
  *runServer)
    echo '[12:00:00] [Server thread/INFO] (Minecraft) Done (1.0s)! For help, type "help"' > "$dir/logs/latest.log"
    exec sleep 60 ;;
  *)
    echo 'client ran' > "$dir/logs/latest.log"
    touch "$dir/options.txt" ;;
esac
"#,
            calls = calls.display()
        ),
    );
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(status.status.success(), "git {args:?}: {status:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    (tmp, root)
}

fn git_status(root: &Path) -> String {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn a_one_shot_run_leaves_git_status_clean() {
    let mut api = Api::new();
    let _files = [
        api.file("sodium", "0.9.3", 1),
        api.file("faithful", "1.0", 1),
    ];
    let (tmp, root) = repo();
    let dir = root.join(RUNS).join(ONE_SHOT);

    run(
        &root,
        &plan(&dir, &["sodium"], &["faithful"], false),
        &pack(),
        &api.registry(),
    )
    .unwrap();

    assert_eq!(git_status(&root), "");
    assert_eq!(
        fs::read_to_string(dir.join("logs/latest.log")).unwrap(),
        "client ran\n"
    );
    assert!(dir.join("mods/sodium-0.9.3.jar").is_file());
    assert_eq!(
        fs::read_to_string(tmp.path().join("calls")).unwrap(),
        format!(":fabric:runClient -Pmod.runDir={}\n", dir.display())
    );
}

#[test]
fn with_a_server_the_client_joins_it_and_the_server_is_stopped_after() {
    let mut api = Api::new();
    let _file = api.file("lithium", "0.26.3", 1);
    let (tmp, root) = repo();
    let dir = root.join(RUNS).join("multi");
    let plan = plan(&dir, &["lithium"], &[], true);
    let started = std::time::Instant::now();

    run(&root, &plan, &pack(), &api.registry()).unwrap();

    // The fake server sleeps a minute unless killed.
    assert!(started.elapsed() < Duration::from_secs(30));
    let properties = fs::read_to_string(dir.join("server/server.properties")).unwrap();
    let port = properties
        .lines()
        .find_map(|l| l.strip_prefix("server-port="))
        .unwrap();
    let server = dir.join("server");
    assert_eq!(
        fs::read_to_string(tmp.path().join("calls")).unwrap(),
        format!(
            ":fabric:runServer -Pmod.runDir={} --console=plain\n\
             :fabric:runClient -Pmod.runDir={} -Pmod.joinServer=localhost:{port}\n",
            server.display(),
            dir.display()
        )
    );
    assert!(server.join("mods/lithium-0.26.3.jar").is_file());
    assert_eq!(git_status(&root), "");
}

/// One RCON packet read from `stream`: its id, type and body.
fn rcon_packet(stream: &mut std::net::TcpStream) -> (i32, i32, String) {
    use std::io::Read;
    let mut len = [0; 4];
    stream.read_exact(&mut len).unwrap();
    let mut rest = vec![0; i32::from_le_bytes(len) as usize];
    stream.read_exact(&mut rest).unwrap();
    let int = |i: usize| i32::from_le_bytes(rest[i..i + 4].try_into().unwrap());
    let body = String::from_utf8(rest[8..rest.len() - 2].to_vec()).unwrap();
    (int(0), int(4), body)
}

fn rcon_reply(stream: &mut std::net::TcpStream, id: i32, kind: i32) {
    use std::io::Write;
    let mut packet = 10i32.to_le_bytes().to_vec();
    packet.extend(id.to_le_bytes());
    packet.extend(kind.to_le_bytes());
    packet.extend([0, 0]);
    stream.write_all(&packet).unwrap();
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .status()
        .unwrap()
        .success()
}

/// The server's wrapper is not the server's process, as on Windows, where it
/// is `cmd.exe` and the server outlives it: once the client exits, the server
/// is stopped through RCON (`stop`, which also saves the world), not by
/// killing the wrapper.
#[test]
fn when_the_client_exits_the_server_is_stopped_through_rcon() {
    let api = Api::new();
    let (_tmp, root) = repo();
    common::write_executable(
        &root.join("gradlew"),
        r#"#!/bin/sh
for arg in "$@"; do
  case "$arg" in -Pmod.runDir=*) dir="${arg#-Pmod.runDir=}" ;; esac
done
mkdir -p "$dir/logs"
case "$1" in
  *runServer)
    sleep 60 &
    echo $! > "$dir/server.pid"
    echo '[12:00:00] [Server thread/INFO] (Minecraft) Done (1.0s)! For help, type "help"' > "$dir/logs/latest.log"
    wait ;;
esac
"#,
    );
    let dir = root.join(RUNS).join("multi");
    let plan = plan(&dir, &[], &[], true);
    let ports = prepare(&plan, &pack(), &api.registry()).unwrap().unwrap();
    let password = fs::read_to_string(dir.join("server/server.properties"))
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("rcon.password=").map(str::to_owned))
        .unwrap();

    // The server's RCON: it stops the server on `stop`.
    let listener = std::net::TcpListener::bind(("127.0.0.1", ports.rcon)).unwrap();
    let pid_file = dir.join("server/server.pid");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let (id, kind, body) = rcon_packet(&mut stream);
        tx.send((kind, body)).unwrap();
        rcon_reply(&mut stream, id, 2);
        let (id, kind, body) = rcon_packet(&mut stream);
        let pid = fs::read_to_string(&pid_file).unwrap();
        Command::new("kill").arg(pid.trim()).status().unwrap();
        rcon_reply(&mut stream, id, 0);
        tx.send((kind, body)).unwrap();
    });

    launch(&root, &plan, Some(ports)).unwrap();

    let pid = fs::read_to_string(dir.join("server/server.pid")).unwrap();
    let pid = pid.trim();
    let server_alive = alive(pid);
    if server_alive {
        Command::new("kill").arg(pid).status().unwrap();
    }
    let received: Vec<_> = rx.try_iter().collect();
    assert!(!server_alive, "the server outlived the run");
    assert_eq!(
        received,
        [(3, password), (2, "stop".to_owned())],
        "RCON login, then `stop`"
    );
}

#[test]
fn a_server_that_stops_before_done_fails_the_run() {
    let api = Api::new();
    let (_tmp, root) = repo();
    common::write_executable(
        &root.join("gradlew"),
        "#!/bin/sh\necho 'BUILD FAILED'\nexit 1\n",
    );
    let dir = root.join(RUNS).join("broken");
    let err = run(&root, &plan(&dir, &[], &[], true), &pack(), &api.registry()).unwrap_err();
    assert!(
        err.to_string()
            .contains("the dev server stopped before `Done`"),
        "{err}"
    );
    assert!(err.hint().contains("gradle.log"), "{}", err.hint());
    assert_eq!(
        fs::read_to_string(dir.join("server/gradle.log")).unwrap(),
        "BUILD FAILED\n"
    );
}

#[test]
fn done_is_read_in_the_log_layout_of_each_loader() {
    for line in [
        "[15:57:21] [Server thread/INFO] (Minecraft) Done (5.856s)! For help, type \"help\"",
        "[15:33:46] [Server thread/INFO] [minecraft/DedicatedServer]: Done (4.802s)! For help, type \"help\"",
    ] {
        assert!(super::is_done(line), "{line}");
    }
    assert!(!super::is_done("[15:33:46] [main/INFO] Done (1s) loading"));
}
