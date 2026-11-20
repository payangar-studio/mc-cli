//! `mod run`: the mod in the game, with extra mods and resource packs from
//! Modrinth, in a run directory under `runs/` (ignored by git).
//!
//! - [`prepare`] resolves what is asked through the shared resolution
//!   ([`mc_core::resolve`]), downloads it into `<run dir>/mods` and
//!   `<run dir>/resourcepacks`, removes what is no longer asked, enables the
//!   resource packs and writes the game options in `options.txt`;
//! - [`launch`] starts the loader's dev client through the repository's
//!   Gradle wrapper, with the run directory as game directory
//!   (`-Pmod.runDir`, read by `build-logic/src/main/kotlin/Runs.kt`). With a
//!   server, it first starts the loader's dev server in `<run dir>/server/`,
//!   waits for its `Done`, then has the client join it
//!   (`-Pmod.joinServer`, `--quickPlayMultiplayer`), and stops it through
//!   RCON when the client exits.

pub mod files;
mod rcon;
#[cfg(test)]
mod tests;

use std::fs;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use mc_core::error::Context as _;
use mc_core::hash::{copy_hashing, hash_file};
use mc_core::model::entry::{Entry, Hashes, Kind, Pack, Support};
use mc_core::platform::modrinth::Modrinth;
use mc_core::platform::{Download, Platform, Registry};

use crate::error::{Error, Result};
use crate::model::mod_toml::Loader;
use crate::model::socle::Versions;
use crate::model::test_profile::Wanted;

/// Directory of the runs, at the repository root, ignored by git.
pub const RUNS: &str = "runs";
/// Run directory of a one-shot run, emptied at its start: a profile name
/// cannot start with a dot.
pub const ONE_SHOT: &str = ".one-shot";
/// Game directory of the dev server, inside the run directory.
pub const SERVER_DIR: &str = "server";
/// Ports the server takes when free; another free port otherwise.
pub const GAME_PORT: u16 = 25565;
pub const RCON_PORT: u16 = 25575;
/// How long the server gets to reach `Done`, its build included.
const SERVER_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// How long the server gets to stop after `stop` before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(60);

/// What a run takes.
#[derive(Debug, Clone)]
pub struct Plan {
    /// The run directory, absolute.
    pub dir: PathBuf,
    pub loader: Loader,
    pub mods: Vec<Wanted>,
    pub packs: Vec<Wanted>,
    /// Game options for `options.txt`.
    pub options: Vec<(String, String)>,
    /// Whether to start a dev server and join it.
    pub server: bool,
}

/// The platforms `mod run` resolves from: Modrinth.
pub fn builtin() -> Registry {
    Registry::new(vec![Box::new(Modrinth::default())])
}

/// What the platforms resolve against: the pinned Minecraft and loader.
pub fn pack(versions: &Versions, loader: Loader) -> Pack {
    Pack {
        minecraft: versions.minecraft.clone(),
        loader: loader.as_str().into(),
        loader_version: match loader {
            Loader::Fabric => versions.fabric_loader.clone(),
            Loader::Neoforge => versions.neoforge.clone(),
        },
    }
}

/// Prepares the run directory, then launches the game from `root`.
pub fn run(root: &Path, plan: &Plan, pack: &Pack, registry: &Registry) -> Result<()> {
    let ports = prepare(plan, pack, registry)?;
    launch(root, plan, ports)
}

/// Ports of the dev server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ports {
    pub game: u16,
    pub rcon: u16,
}

/// Fills the run directory (and the server's, with a server); returns the
/// server's ports.
pub fn prepare(plan: &Plan, pack: &Pack, registry: &Registry) -> Result<Option<Ports>> {
    fs::create_dir_all(&plan.dir).context(format!("creating {}", plan.dir.display()))?;
    let mods = resolve_all(registry, pack, &plan.mods, Kind::Mod)?;
    let packs = resolve_all(registry, pack, &plan.packs, Kind::Resourcepack)?;
    let on = |side: fn(&Entry) -> Support| -> Vec<&(&dyn Platform, Entry)> {
        mods.iter()
            .filter(|(_, e)| side(e) != Support::Unsupported)
            .collect()
    };
    let client_mods = plan.dir.join(Kind::Mod.dir());
    place(&client_mods, Kind::Mod, &on(|e| e.env.client), None)?;
    let packs: Vec<_> = packs.iter().collect();
    place(
        &plan.dir.join(Kind::Resourcepack.dir()),
        Kind::Resourcepack,
        &packs,
        None,
    )?;
    let pack_names: Vec<&str> = packs.iter().map(|(_, e)| file_name(e)).collect();
    files::write_options(&plan.dir, &pack_names, &plan.options)?;

    if !plan.server {
        return Ok(None);
    }
    let server = plan.dir.join(SERVER_DIR);
    place(
        &server.join(Kind::Mod.dir()),
        Kind::Mod,
        &on(|e| e.env.server),
        Some(&client_mods),
    )?;
    let ports = Ports {
        game: free_port(GAME_PORT)?,
        rcon: free_port(RCON_PORT)?,
    };
    files::write_server_properties(&server, ports)?;
    Ok(Some(ports))
}

/// The entries of `wanted`, each with the platform that knows it.
fn resolve_all<'a>(
    registry: &'a Registry,
    pack: &Pack,
    wanted: &[Wanted],
    kind: Kind,
) -> Result<Vec<(&'a dyn Platform, Entry)>> {
    wanted
        .iter()
        .map(|w| resolve(registry, pack, w, kind))
        .collect()
}

/// Makes `dir` hold the files of `entries` and no other file of `kind`:
/// a file already there with the same content is kept, one in `from` is
/// linked, any other is downloaded.
fn place(
    dir: &Path,
    kind: Kind,
    entries: &[&(&dyn Platform, Entry)],
    from: Option<&Path>,
) -> Result<()> {
    let same = |path: &Path, entry: &Entry| {
        hash_file(path).is_ok_and(|h| h.sha512.eq_ignore_ascii_case(&entry.hashes.sha512))
    };
    let mut names = Vec::new();
    for (platform, entry) in entries {
        let name = file_name(entry);
        let target = dir.join(name);
        if !same(&target, entry) {
            let _ = fs::remove_file(&target);
            match from.map(|d| d.join(name)).filter(|p| same(p, entry)) {
                Some(source) => {
                    fs::create_dir_all(dir).context(format!("creating {}", dir.display()))?;
                    mc_core::server::link_or_copy(&source, &target).map_err(|reason| {
                        Error::internal(reason, "check the permissions of the run directory")
                    })?
                }
                None => download(*platform, entry, &target)?,
            }
        }
        println!("{} {}", kind.noun(), target.display());
        names.push(name);
    }
    files::keep_only(dir, kind, &names)
}

/// The entry of `wanted`, completed by the platform that knows it.
fn resolve<'a>(
    registry: &'a Registry,
    pack: &Pack,
    wanted: &Wanted,
    kind: Kind,
) -> Result<(&'a dyn Platform, Entry)> {
    let (platform, candidates) = mc_core::resolve::resolve(registry, None, &wanted.slug, pack)?;
    if let Some(other) = candidates.iter().map(|c| c.kind).find(|k| *k != kind) {
        let option = match other {
            Kind::Mod => "--with",
            Kind::Resourcepack => "--pack",
        };
        return Err(Error::usage(
            format!(
                "`{}` is a {}, not a {}",
                wanted.slug,
                other.noun(),
                kind.noun()
            ),
            format!("pass it with `{option}`, or under the matching key of the test profile"),
        ));
    }
    let candidate = match &wanted.version {
        None => &candidates[0],
        Some(version) => candidates
            .iter()
            .find(|c| {
                c.label == *version
                    || c.section.get("version_id").and_then(toml::Value::as_str)
                        == Some(version.as_str())
            })
            .ok_or_else(|| {
                let shown: Vec<&str> = candidates
                    .iter()
                    .take(10)
                    .map(|c| c.label.as_str())
                    .collect();
                Error::usage(
                    format!(
                        "`{}` has no version `{version}` for Minecraft {} with {}",
                        wanted.slug, pack.minecraft, pack.loader
                    ),
                    format!("pick one of: {}", shown.join(", ")),
                )
            })?,
    };
    let mut entry = Entry {
        key: wanted.slug.clone(),
        kind,
        path: format!("{}/{}", kind.dir(), candidate.file_name),
        file_size: 0,
        downloads: Vec::new(),
        url: None,
        source: None,
        approved: None,
        hashes: Hashes {
            sha1: String::new(),
            sha512: String::new(),
        },
        env: kind.default_env(),
        platforms: [(platform.name().to_owned(), candidate.section.clone())].into(),
    };
    platform.fill_entry(&mut entry)?;
    if kind == Kind::Resourcepack {
        entry.env = kind.default_env();
    }
    Ok((platform, entry))
}

/// Downloads the file of `entry` to `target`, checking its sha512.
fn download(platform: &dyn Platform, entry: &Entry, target: &Path) -> Result<()> {
    let mut reader = match platform.download(entry)? {
        Download::Stream(stream) => stream,
        Download::Url(url) => mc_core::fetch::Fetch::get(&mc_core::fetch::Http::new(), &url)?,
    };
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).context(format!("creating {}", dir.display()))?;
    }
    let part = target.with_extension(files::PART);
    let mut file = fs::File::create(&part).context(format!("writing {}", part.display()))?;
    let hashes = copy_hashing(&mut reader, &mut file);
    drop(file);
    let hashes = match hashes {
        Ok(hashes) => hashes,
        Err(e) => {
            let _ = fs::remove_file(&part);
            return Err(Error::internal(
                format!("cannot download `{}`: {e}", entry.key),
                "run the command again; if it persists, check https://status.modrinth.com",
            ));
        }
    };
    if !hashes.sha512.eq_ignore_ascii_case(&entry.hashes.sha512) {
        let _ = fs::remove_file(&part);
        return Err(Error::internal(
            format!("the download of `{}` does not match its sha512", entry.key),
            "run the command again",
        ));
    }
    fs::rename(&part, target).context(format!("writing {}", target.display()))
}

fn file_name(entry: &Entry) -> &str {
    entry.path.rsplit('/').next().unwrap_or(&entry.path)
}

/// `preferred` when free on this machine, else any free port.
fn free_port(preferred: u16) -> Result<u16> {
    if TcpListener::bind(("127.0.0.1", preferred)).is_ok() {
        return Ok(preferred);
    }
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .context("finding a free port")
}

/// The repository's Gradle wrapper, running `task` of the loader's
/// subproject with the game directory `dir`.
fn gradle(root: &Path, loader: Loader, task: &str, dir: &Path) -> Command {
    let wrapper = if cfg!(windows) {
        "gradlew.bat"
    } else {
        "gradlew"
    };
    let mut command = Command::new(root.join(wrapper));
    command
        .current_dir(root)
        .arg(format!(":{}:{task}", loader.as_str()))
        .arg(format!("-Pmod.runDir={}", dir.display()));
    command
}

/// Starts the game as `plan` says, in the directory `prepare` filled.
pub fn launch(root: &Path, plan: &Plan, ports: Option<Ports>) -> Result<()> {
    let mut client = gradle(root, plan.loader, "runClient", &plan.dir);
    let server = match ports {
        Some(ports) => {
            let server = start_server(root, plan, ports)?;
            client.arg(format!("-Pmod.joinServer=localhost:{}", ports.game));
            Some(server)
        }
        None => None,
    };
    println!("Run directory: {}", plan.dir.display());
    let status = client.status();
    if let (Some(mut server), Some(ports)) = (server, ports) {
        stop_server(&mut server, &plan.dir.join(SERVER_DIR), ports);
    }
    let status = status.context("starting the Gradle wrapper")?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::internal(
            format!("the game stopped with {status}"),
            format!(
                "read {} and the output above",
                plan.dir.join("logs/latest.log").display()
            ),
        ))
    }
}

/// Starts the dev server in the background and waits for its `Done`. Its
/// output goes to `<server dir>/gradle.log`.
fn start_server(root: &Path, plan: &Plan, ports: Ports) -> Result<Child> {
    let dir = plan.dir.join(SERVER_DIR);
    let log = dir.join("logs/latest.log");
    let _ = fs::remove_file(&log);
    let output = dir.join("gradle.log");
    let out = fs::File::create(&output).context(format!("writing {}", output.display()))?;
    let err = out
        .try_clone()
        .context(format!("writing {}", output.display()))?;
    let mut child = gradle(root, plan.loader, "runServer", &dir)
        .arg("--console=plain")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .context("starting the Gradle wrapper")?;
    println!(
        "Starting the dev server in {} (log: {})",
        dir.display(),
        output.display()
    );
    let deadline = Instant::now() + SERVER_TIMEOUT;
    loop {
        if done(&log) {
            break;
        }
        let stopped = child.try_wait().ok().flatten();
        if stopped.is_some() || Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            let what = match stopped {
                Some(status) => format!("the dev server stopped before `Done` ({status})"),
                None => format!(
                    "the dev server did not reach `Done` within {} minutes",
                    SERVER_TIMEOUT.as_secs() / 60
                ),
            };
            return Err(Error::internal(
                what,
                format!("read {} and {}", output.display(), log.display()),
            ));
        }
        thread::sleep(Duration::from_millis(500));
    }
    println!(
        "Dev server on localhost:{} (offline mode), RCON on localhost:{} with the password in {}",
        ports.game,
        ports.rcon,
        dir.join("server.properties").display()
    );
    Ok(child)
}

/// Stops the dev server whose Gradle wrapper is `server`: `stop` through RCON,
/// which saves the world, then waits for the wrapper to exit. Killing the
/// wrapper is the fallback, when RCON fails or the server does not stop in
/// time: on Windows the wrapper is `cmd.exe`, and the server outlives it.
fn stop_server(server: &mut Child, dir: &Path, ports: Ports) {
    let sent = match files::rcon_password(dir) {
        Some(password) => rcon::command(ports.rcon, &password, "stop"),
        None => Err(std::io::Error::other(
            "no RCON password in server.properties",
        )),
    };
    match sent {
        Ok(()) => {
            let deadline = Instant::now() + STOP_GRACE;
            while Instant::now() < deadline {
                if let Ok(Some(_)) = server.try_wait() {
                    return;
                }
                thread::sleep(Duration::from_millis(200));
            }
            tracing::warn!(
                "the dev server did not stop within {}s of `stop`, killed",
                STOP_GRACE.as_secs()
            );
        }
        Err(e) => tracing::warn!("cannot stop the dev server through RCON ({e}), killed"),
    }
    let _ = server.kill();
    let _ = server.wait();
}

/// Whether the server log at `log` has its `Done` line, in the layout of
/// either loader's dev runs:
/// `[12:00:00] [Server thread/INFO] (Minecraft) Done (4.1s)! For help, type "help"`
/// on Fabric, `... [minecraft/DedicatedServer]: Done (4.8s)! ...` on NeoForge.
fn done(log: &Path) -> bool {
    let Ok(file) = fs::File::open(log) else {
        return false;
    };
    BufReader::new(file)
        .lines()
        .map_while(std::result::Result::ok)
        .any(|line| is_done(&line))
}

fn is_done(line: &str) -> bool {
    line.contains(" Done (") && line.contains("For help, type")
}
