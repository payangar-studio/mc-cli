//! The side matrix: a dedicated server with one mod set and a client with
//! another, in two processes, since no loader starts both sides with
//! different mods. The cases follow the side in `mod.toml` ([`matrix`]).
//!
//! A case starts the loader's dedicated server (installed once into the
//! machine cache by [`mc_core::server`]) with or without the mod, waits for
//! its `Done`, then launches a client ([`client`]) that joins it directly.
//! The verdict comes from the server's log: the player joined and is still
//! there some ticks later, or was refused with the expected message. Then
//! both stop. Each case writes its logs under `runs/sides/<loader>/<case>/`.

pub mod client;
pub mod matrix;

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use mc_core::fetch::Fetch;
use mc_core::model::entry::Pack;
use mc_core::server::loader::Loader as ServerLoader;
use mc_core::server::run::{Running, Wait, is_done};
use mc_core::server::{Machine, Step, download, free_port, installed, io_err, link_or_copy};

use crate::model::mod_toml::{Loader, ModToml};
use crate::model::socle::Versions;
use client::{Flavour, Launcher};
use matrix::{Case, Client, Expect};

/// How long a server gets to reach `Done`.
const SERVER_TIMEOUT: Duration = Duration::from_secs(300);
/// How long a client gets to join or be refused: the first launch downloads
/// the game and its assets.
const JOIN_TIMEOUT: Duration = Duration::from_secs(600);
/// How long the server gets to answer a console command.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
/// Where the runs of the matrix write, under the repository.
pub const RUNS_DIR: &str = "runs/sides";
/// The version the matrix builds the mod at (`-Pversion`), which names the
/// jar under test: a jar of another version left in `build/libs` is not it.
pub const BUILD_VERSION: &str = "0.0.0";

/// What every case of a loader shares.
pub struct Setup<'a> {
    pub module: &'a ModToml,
    pub versions: &'a Versions,
    pub loader: Loader,
    /// The mod's jar and the jars of its dependencies.
    pub mods: Vec<PathBuf>,
    pub machine: &'a Machine<'a>,
    pub java_major: u32,
    /// HeadlessMC, when a case launches a client.
    pub launcher: Option<&'a Launcher>,
    /// How long a client stays before the check that it is still there.
    pub stay: Duration,
    /// The directory of this loader's runs.
    pub runs: PathBuf,
}

impl Setup<'_> {
    fn pack(&self) -> Pack {
        let loader_version = match self.loader {
            Loader::Fabric => &self.versions.fabric_loader,
            Loader::Neoforge => &self.versions.neoforge,
        };
        Pack {
            minecraft: self.versions.minecraft.clone(),
            loader: self.loader.as_str().into(),
            loader_version: loader_version.clone(),
        }
    }

    fn server_loader(&self) -> ServerLoader {
        match self.loader {
            Loader::Fabric => ServerLoader::Fabric,
            Loader::Neoforge => ServerLoader::NeoForge,
        }
    }

    fn modded_client(&self) -> Flavour {
        match self.loader {
            Loader::Fabric => Flavour::Fabric {
                loader: self.versions.fabric_loader.clone(),
            },
            Loader::Neoforge => Flavour::NeoForge {
                version: self.versions.neoforge.clone(),
            },
        }
    }
}

/// Runs `case`; the error is the reason of its failure. The logs of both
/// sides stay in its run directory.
pub fn run_case(setup: &Setup<'_>, case: &Case, fetch: &dyn Fetch) -> Step<()> {
    let dir = setup.runs.join(case.name);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(io_err("removing", &dir))?;
    }
    fs::create_dir_all(&dir).map_err(io_err("creating", &dir))?;

    let pack = setup.pack();
    let server_dir = installed(
        setup.server_loader(),
        &pack,
        setup.java_major,
        setup.machine,
    )?;
    let run = mc_core::server::run_dir(
        &server_dir,
        &format!("{}-{}", setup.loader, case.name),
        setup.machine,
    )?;
    let port = prepare_server(run.path(), case.server_has_mod.then_some(&setup.mods[..]))?;
    let mut command = Command::new(&setup.machine.java);
    command
        .args(setup.server_loader().launch_args(&pack))
        .current_dir(run.path());
    tracing::info!(loader = %setup.loader, case = case.name, "starting the server");
    let mut server = Running::start(command, Some(&dir.join("server.log")))?;
    let result = play(setup, case, &mut server, port, &dir, fetch);
    server.stop();
    result
}

/// The case once the server is started.
fn play(
    setup: &Setup<'_>,
    case: &Case,
    server: &mut Running,
    port: u16,
    dir: &Path,
    fetch: &dyn Fetch,
) -> Step<()> {
    let id = &setup.module.id;
    let mut listed = false;
    server
        .wait_for(SERVER_TIMEOUT, |l| {
            listed |= lists_mod(l, id);
            is_done(l)
        })
        .map_err(|wait| server_failed(server, &wait, "reaching `Done`"))?;
    if case.server_loads_mod && !listed {
        return Err(format!(
            "the server did not load the mod `{id}`; last lines of its log:\n{}",
            server.tail()
        ));
    }

    let flavour = match case.client {
        Client::None => {
            return match server.wait_for(setup.stay, |_| false) {
                Err(Wait::Timeout) => Ok(()),
                Err(wait) => Err(server_failed(server, &wait, "running")),
                Ok(_) => unreachable!("no line is awaited"),
            };
        }
        Client::Vanilla => Flavour::Vanilla,
        Client::Modded => setup.modded_client(),
    };
    let launcher = setup
        .launcher
        .ok_or("the case launches a client, but HeadlessMC was not installed")?;
    let target = launcher.target(&flavour, fetch)?;
    let game_dir = dir.join("client");
    if case.client == Client::Modded {
        copy_mods(&game_dir.join("mods"), &setup.mods)?;
    }
    tracing::info!(loader = %setup.loader, case = case.name, ?target, "launching the client");
    let mut game = launcher.launch(
        &target,
        &game_dir,
        &dir.join("headlessmc"),
        &format!("127.0.0.1:{port}"),
        &dir.join("client.log"),
    )?;

    // The client joins, or is refused.
    let deadline = Instant::now() + JOIN_TIMEOUT;
    let line = loop {
        match server.wait_for(Duration::from_secs(1), |l| joined(l) || lost(l).is_some()) {
            Ok(line) => break line,
            Err(Wait::Timeout) => {}
            Err(wait) => return Err(server_failed(server, &wait, "waiting for the client")),
        }
        if let Some(failure) = game.failed() {
            return Err(format!(
                "{failure} before reaching the server; last lines of its output:\n{}",
                game.tail()
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "the client did not reach the server within {}s; last lines of its output:\n{}",
                JOIN_TIMEOUT.as_secs(),
                game.tail()
            ));
        }
    };

    let result = match case.expect {
        Expect::Refused => refused(&line, &setup.module.name),
        Expect::Plays => stays(setup, server, &line),
        Expect::Runs => unreachable!("a case with a client expects it to join or be refused"),
    };
    game.stop();
    result?;
    if case.client == Client::Modded {
        let log = game_dir.join("logs/latest.log");
        let text = fs::read_to_string(&log).map_err(io_err("reading", &log))?;
        if !text.lines().any(|l| lists_mod(l, id)) {
            return Err(format!(
                "the client did not load the mod `{id}` (no such entry in {})",
                log.display()
            ));
        }
    }
    Ok(())
}

/// The client joined: it must still be there after the stay.
fn stays(setup: &Setup<'_>, server: &mut Running, line: &str) -> Step<()> {
    if let Some(reason) = lost(line) {
        return Err(format!("the server refused the client: {reason}"));
    }
    match server.wait_for(setup.stay, |l| {
        lost(l).is_some() || l.contains(" left the game")
    }) {
        Err(Wait::Timeout) => {}
        Ok(line) => {
            return Err(format!(
                "the client left within {} ticks: {line}",
                setup.stay.as_millis() / 50
            ));
        }
        Err(wait) => return Err(server_failed(server, &wait, "the client played")),
    }
    server.send("list")?;
    let list = server
        .wait_for(COMMAND_TIMEOUT, |l| l.contains(" players online"))
        .map_err(|wait| server_failed(server, &wait, "answering `list`"))?;
    if list.contains("There are 0 ") {
        return Err(format!("the client is no longer on the server: {list}"));
    }
    Ok(())
}

/// The server must have refused the client with the side check's message.
fn refused(line: &str, name: &str) -> Step<()> {
    let expected = format!("This server requires the mod {name}");
    match lost(line) {
        Some(reason) if reason.contains(&expected) => Ok(()),
        Some(reason) => Err(format!(
            "the server refused the client with `{reason}`; expected `{expected}`"
        )),
        None => Err(format!(
            "the client joined ({line}); expected a refusal with `{expected}`"
        )),
    }
}

fn server_failed(server: &Running, wait: &Wait, doing: &str) -> String {
    match wait {
        Wait::Timeout => format!(
            "the server timed out {doing}; last lines of its log:\n{}",
            server.tail()
        ),
        Wait::Exited(status) => format!(
            "the server stopped ({status}) while {doing}; last lines of its log:\n{}",
            server.tail()
        ),
    }
}

/// `[Server thread/INFO]: Offline joined the game`
fn joined(line: &str) -> bool {
    line.contains(" joined the game")
}

/// The reason of `<player> lost connection: <reason>`.
fn lost(line: &str) -> Option<&str> {
    line.split_once(" lost connection: ")
        .map(|(_, reason)| reason)
}

/// Whether `line` is the entry of the mod `id` in a loader's list of mods:
/// `\t- <id> <version>` for Fabric, `\t\t<Name> <version> (<id>)` for
/// NeoForge.
fn lists_mod(line: &str, id: &str) -> bool {
    let line = line.trim();
    line.strip_prefix("- ")
        .is_some_and(|rest| rest.split(' ').next() == Some(id))
        || line.ends_with(&format!("({id})"))
}

/// Accepts the EULA, writes the settings a test server needs and puts `mods`
/// into `mods/`. Returns the server's port.
fn prepare_server(dir: &Path, mods: Option<&[PathBuf]>) -> Step<u16> {
    if let Some(mods) = mods {
        copy_mods(&dir.join("mods"), mods)?;
    }
    let eula = dir.join("eula.txt");
    fs::write(&eula, "eula=true\n").map_err(io_err("writing", &eula))?;
    let port = free_port()?;
    let properties = dir.join("server.properties");
    fs::write(&properties, server_properties(port)).map_err(io_err("writing", &properties))?;
    Ok(port)
}

/// Offline players, none of them refused by a whitelist (on by default in
/// 26.3) or for an unsigned profile; a flat world without structures, quick
/// to generate.
fn server_properties(port: u16) -> String {
    format!(
        "server-ip=127.0.0.1\nserver-port={port}\nonline-mode=false\nwhite-list=false\n\
         enforce-secure-profile=false\nlevel-type=minecraft\\:flat\ngenerate-structures=false\n"
    )
}

fn copy_mods(dir: &Path, mods: &[PathBuf]) -> Step<()> {
    fs::create_dir_all(dir).map_err(io_err("creating", dir))?;
    for jar in mods {
        let name = jar.file_name().ok_or("a mod jar has no file name")?;
        link_or_copy(jar, &dir.join(name))?;
    }
    Ok(())
}

/// The jars of the dependencies the mod requires on `loader`. Fabric API
/// comes from Fabric's maven at the socle's version; another required
/// dependency is not fetched yet.
pub fn dependencies(
    module: &ModToml,
    versions: &Versions,
    loader: Loader,
    cache: &Path,
    fetch: &dyn Fetch,
) -> Step<Vec<PathBuf>> {
    let mut jars = Vec::new();
    for dependency in &module.dependencies {
        let applies = dependency
            .loaders
            .as_ref()
            .is_none_or(|loaders| loaders.contains(&loader));
        if !applies || dependency.kind != "required" {
            continue;
        }
        if loader == Loader::Fabric && dependency.id == "fabric-api" {
            jars.push(fabric_api(&versions.fabric_api, cache, fetch)?);
            continue;
        }
        return Err(format!(
            "the side matrix cannot fetch the required dependency `{}` yet: only Fabric API is fetched",
            dependency.id
        ));
    }
    Ok(jars)
}

/// Fabric API `version`, downloaded once into `cache` and checked against
/// the SHA-1 its maven publishes.
fn fabric_api(version: &str, cache: &Path, fetch: &dyn Fetch) -> Step<PathBuf> {
    let dir = cache.join("jars");
    let jar = dir.join(format!("fabric-api-{version}.jar"));
    if jar.is_file() {
        return Ok(jar);
    }
    fs::create_dir_all(&dir).map_err(io_err("creating", &dir))?;
    let url = format!(
        "https://maven.fabricmc.net/net/fabricmc/fabric-api/fabric-api/{version}/fabric-api-{version}.jar"
    );
    let partial = download_beside(fetch, &url, &jar)?;
    let mut expected = String::new();
    fetch
        .get(&format!("{url}.sha1"))
        .map_err(mc_core::server::reason)?
        .read_to_string(&mut expected)
        .map_err(|e| format!("cannot read {url}.sha1: {e}"))?;
    let got = mc_core::hash::hash_file(&partial).map_err(io_err("reading", &partial))?;
    if got.sha1 != expected.trim() {
        return Err(format!("{url} does not match its published SHA-1"));
    }
    partial
        .persist(&jar)
        .map_err(|e| io_err("writing", &jar)(e.error))?;
    Ok(jar)
}

/// Downloads `url` into a file of its own in the directory of `to`, removed
/// when dropped unless persisted to `to`: runs sharing the machine cache never
/// write the same file.
pub fn download_beside(fetch: &dyn Fetch, url: &str, to: &Path) -> Step<tempfile::TempPath> {
    let dir = to.parent().ok_or("a download has no directory")?;
    let partial = tempfile::Builder::new()
        .prefix(".download-")
        .tempfile_in(dir)
        .map_err(io_err("creating a file in", dir))?
        .into_temp_path();
    download(fetch, url, &partial)?;
    Ok(partial)
}

/// The mod's jar built for `loader` at [`BUILD_VERSION`]:
/// `<loader>/build/libs/<id>-<loader>-<version>.jar`.
pub fn mod_jar(root: &Path, id: &str, loader: Loader) -> Step<PathBuf> {
    let jar = root
        .join(loader.as_str())
        .join("build/libs")
        .join(format!("{id}-{loader}-{BUILD_VERSION}.jar"));
    if !jar.is_file() {
        return Err(format!("the build made no {}", jar.display()));
    }
    Ok(jar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_lists_of_both_loaders_are_read() {
        assert!(lists_mod("\t- swift_blades 0.0.0", "swift_blades"));
        assert!(lists_mod(
            "\t\tSwift Blades 0.0.0 (swift_blades)",
            "swift_blades"
        ));
        assert!(!lists_mod(
            "\t- swift_blades_gametest 0.0.0",
            "swift_blades"
        ));
        assert!(!lists_mod("\t   |-- fabric-api-base 2.0.6", "fabric-api"));
        assert!(!lists_mod(
            "\t\tSwift Blades 0.0.0 (swift_blades_gametest)",
            "swift_blades"
        ));
    }

    #[test]
    fn joins_and_refusals_are_read_from_the_server_log() {
        assert!(joined(
            "[18:06:34] [Server thread/INFO]: System chat: Offline joined the game"
        ));
        assert_eq!(
            lost(
                "[Server thread/INFO]: Offline lost connection: This server requires the mod Swift Blades. Install it to join."
            ),
            Some("This server requires the mod Swift Blades. Install it to join.")
        );
        assert_eq!(lost("Offline left the game"), None);
    }

    #[test]
    fn a_refusal_must_carry_the_side_check_message() {
        let line =
            |reason: &str| format!("[Server thread/INFO]: Offline lost connection: {reason}");
        assert_eq!(
            refused(
                &line("This server requires the mod Swift Blades. Install it to join."),
                "Swift Blades"
            ),
            Ok(())
        );
        let other = refused(&line("Disconnected"), "Swift Blades").unwrap_err();
        assert!(other.contains("`Disconnected`"), "{other}");
        let joined = refused("Offline joined the game", "Swift Blades").unwrap_err();
        assert!(joined.starts_with("the client joined"), "{joined}");
    }

    #[test]
    fn the_test_server_accepts_offline_players_on_its_port() {
        let properties = server_properties(25601);
        for line in [
            "server-port=25601",
            "online-mode=false",
            "white-list=false",
            "enforce-secure-profile=false",
        ] {
            assert!(properties.lines().any(|l| l == line), "{line}");
        }
    }

    #[test]
    fn the_jar_of_the_build_version_is_the_mod() {
        let root = tempfile::tempdir().unwrap();
        let libs = root.path().join("fabric/build/libs");
        fs::create_dir_all(&libs).unwrap();
        for name in [
            "swift_blades-fabric-0.0.0-sources.jar",
            "swift_blades-fabric-0.0.0.jar",
            // A publication's jar, newer.
            "swift_blades-fabric-1.2.0.jar",
        ] {
            fs::write(libs.join(name), "").unwrap();
        }
        assert_eq!(
            mod_jar(root.path(), "swift_blades", Loader::Fabric).unwrap(),
            libs.join("swift_blades-fabric-0.0.0.jar")
        );
        let missing = mod_jar(root.path(), "swift_blades", Loader::Neoforge).unwrap_err();
        assert!(missing.starts_with("the build made no "), "{missing}");
    }

    #[test]
    fn only_required_dependencies_of_the_loader_are_fetched() {
        let module: ModToml = toml::from_str(
            r#"
id = "swift_blades"
name = "Swift Blades"
package = "com.payangar.swiftblades"
side = "both-required"
loaders = ["fabric", "neoforge"]
license = "MIT"

[[dependencies]]
id = "sodium"
type = "optional"

[[dependencies]]
id = "jei"
type = "required"
loaders = ["neoforge"]
"#,
        )
        .unwrap();
        let versions = crate::versions::SUPPORTED[0].versions();
        let cache = tempfile::tempdir().unwrap();
        let fetch = mc_core::fetch::fake::NoFetch;
        assert_eq!(
            dependencies(&module, &versions, Loader::Fabric, cache.path(), &fetch).unwrap(),
            Vec::<PathBuf>::new()
        );
        let jei =
            dependencies(&module, &versions, Loader::Neoforge, cache.path(), &fetch).unwrap_err();
        assert!(jei.contains("`jei`"), "{jei}");
    }
}
