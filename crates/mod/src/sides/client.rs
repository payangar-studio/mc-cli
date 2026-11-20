//! The game client of the side matrix, launched by HeadlessMC without a
//! display: the vanilla client, or a loader's client with a mod set in its
//! `mods/`. HeadlessMC patches LWJGL so that nothing is rendered, and plays
//! with an offline account, which its licence allows for headless runs in CI.
//!
//! HeadlessMC is a jar run by the Java of the game, downloaded once into the
//! machine cache and checked against its pinned hash. Versions, libraries and
//! assets go to a Minecraft directory shared by the runs; each run has its
//! own HeadlessMC settings and game directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use mc_core::fetch::Fetch;
use mc_core::server::loader::NEOFORGE_MAVEN;
use mc_core::server::{Step, download, io_err, run};

use super::download_beside;

/// The pinned HeadlessMC release: 3.0.0 is still a release candidate, and
/// RC9 breaks compatibility with RC8.
pub const HEADLESSMC_VERSION: &str = "3.0.0-RC9";
/// SHA-256 of `headlessmc.jar` of [`HEADLESSMC_VERSION`], as GitHub lists it.
const HEADLESSMC_SHA256: &str = "a343a7c51e48c566a5cd408bd0012ddf091ea105ecc31daff00af600d3a7218a";
/// How long a stopped client gets before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(30);
/// How long a killed client gets to be gone.
const KILL_WAIT: Duration = Duration::from_secs(10);

/// The process groups of the clients still running, for [`kill_running`].
static RUNNING: Mutex<Vec<u32>> = Mutex::new(Vec::new());

fn running() -> MutexGuard<'static, Vec<u32>> {
    RUNNING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Kills every client still running, for a run that is interrupted: in their
/// own process group, Ctrl-C does not reach them, and a vanilla client never
/// quits by itself.
pub fn kill_running() {
    for group in running().drain(..) {
        signal_group(group, "KILL");
    }
}

/// The client to launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flavour {
    Vanilla,
    Fabric { loader: String },
    NeoForge { version: String },
}

/// HeadlessMC, ready to launch clients of `minecraft`.
pub struct Launcher {
    java: PathBuf,
    java_home: PathBuf,
    jar: PathBuf,
    /// The Minecraft directory shared by the runs.
    minecraft_dir: PathBuf,
    minecraft: String,
}

impl Launcher {
    /// HeadlessMC from `cache` (`<cache>/headlessmc/<version>/`), downloaded
    /// first if needed, with the shared Minecraft directory `<cache>/minecraft`.
    pub fn install(java: &Path, minecraft: &str, cache: &Path, fetch: &dyn Fetch) -> Step<Self> {
        let dir = cache.join("headlessmc").join(HEADLESSMC_VERSION);
        let jar = dir.join("headlessmc.jar");
        if !sha256_matches(&jar)? {
            fs::create_dir_all(&dir).map_err(io_err("creating", &dir))?;
            let partial = download_beside(
                fetch,
                &format!(
                    "https://github.com/headlesshq/headlessmc/releases/download/{HEADLESSMC_VERSION}/headlessmc.jar"
                ),
                &jar,
            )?;
            if !sha256_matches(&partial)? {
                return Err(format!(
                    "the downloaded headlessmc.jar {HEADLESSMC_VERSION} does not have its published SHA-256"
                ));
            }
            partial
                .persist(&jar)
                .map_err(|e| io_err("writing", &jar)(e.error))?;
        }
        Ok(Self {
            java: java.to_owned(),
            java_home: java_home(java)?,
            jar,
            minecraft_dir: cache.join("minecraft"),
            minecraft: minecraft.to_owned(),
        })
    }

    /// The arguments of `launch` for `flavour`, installing its loader first
    /// when HeadlessMC cannot.
    pub fn target(&self, flavour: &Flavour, fetch: &dyn Fetch) -> Step<Vec<String>> {
        Ok(match flavour {
            Flavour::Vanilla => vec![self.minecraft.clone()],
            Flavour::Fabric { loader } => {
                vec!["fabric".into(), self.minecraft.clone(), loader.clone()]
            }
            // HeadlessMC does not resolve the builds of NeoForge 26.x: its
            // installer installs the client, launched by its id.
            Flavour::NeoForge { version } => {
                let id = format!("neoforge-{version}");
                let installed = self.minecraft_dir.join("versions").join(&id);
                let is_installed = || installed.join(format!("{id}.json")).is_file();
                if !is_installed() {
                    // Runs sharing the Minecraft directory install one at a time.
                    let _lock = self.lock("neoforge-install")?;
                    if !is_installed() {
                        self.install_neoforge(version, fetch)?;
                    }
                }
                vec![id]
            }
        })
    }

    /// Holds the lock `name` of the shared Minecraft directory until dropped.
    fn lock(&self, name: &str) -> Step<fs::File> {
        let dir = &self.minecraft_dir;
        fs::create_dir_all(dir).map_err(io_err("creating", dir))?;
        let path = dir.join(format!(".{name}.lock"));
        let file = fs::File::create(&path).map_err(io_err("creating", &path))?;
        file.lock().map_err(io_err("locking", &path))?;
        Ok(file)
    }

    fn install_neoforge(&self, version: &str, fetch: &dyn Fetch) -> Step<()> {
        let dir = &self.minecraft_dir;
        fs::create_dir_all(dir).map_err(io_err("creating", dir))?;
        // The installer wants the file of a launcher.
        let profiles = dir.join("launcher_profiles.json");
        if !profiles.is_file() {
            fs::write(&profiles, "{\"profiles\":{}}\n").map_err(io_err("writing", &profiles))?;
        }
        // The installer under its own name, in a directory of this run.
        let staging = tempfile::Builder::new()
            .prefix(".neoforge-installer-")
            .tempdir_in(dir)
            .map_err(io_err("creating a directory in", dir))?;
        let installer = staging
            .path()
            .join(format!("neoforge-{version}-installer.jar"));
        download(
            fetch,
            &format!("{NEOFORGE_MAVEN}/{version}/neoforge-{version}-installer.jar"),
            &installer,
        )?;
        tracing::info!(version, "installing the NeoForge client");
        let output = Command::new(&self.java)
            .arg("-jar")
            .arg(&installer)
            .arg("--install-client")
            .arg(dir)
            .current_dir(dir)
            .output()
            .map_err(|e| format!("cannot run Java (`{}`): {e}", self.java.display()))?;
        drop(staging);
        if output.status.success() {
            return Ok(());
        }
        Err(format!(
            "the NeoForge client installer failed ({}); last lines of its output:\n{}",
            output.status,
            run::last_lines(&format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        ))
    }

    /// Launches `target` in the game directory `game`, straight to the server
    /// at `server` (`host:port`), HeadlessMC's settings in `files` and its
    /// output in `log`.
    pub fn launch(
        &self,
        target: &[String],
        game: &Path,
        files: &Path,
        server: &str,
        log: &Path,
    ) -> Step<Game> {
        if !cfg!(unix) {
            return Err(
                "the side matrix stops the client through its process group: Linux and macOS only"
                    .into(),
            );
        }
        fs::create_dir_all(files).map_err(io_err("creating", files))?;
        fs::create_dir_all(game).map_err(io_err("creating", game))?;
        let config = files.join("config.properties");
        fs::write(&config, self.config(game)).map_err(io_err("writing", &config))?;
        // The first launch would open the accessibility screen, and a game
        // that loses the focus would pause.
        let options = game.join("options.txt");
        if !options.is_file() {
            fs::write(
                &options,
                "onboardAccessibility:false\npauseOnLostFocus:false\nnarrator:0\n",
            )
            .map_err(io_err("writing", &options))?;
        }
        let mut command = Command::new(&self.java);
        command
            .arg("-jar")
            .arg(&self.jar)
            .args(["launch", "-lwjgl", "--offline", "--no-backup", "--game"])
            .arg(format!("--quickPlayMultiplayer {server}"))
            .args(target)
            .env("HMC_FILES_LOCATION", files)
            .current_dir(game);
        Game::spawn(command, log)
    }

    /// HeadlessMC's settings: the Java of the game, never one it downloads;
    /// the shared Minecraft directory; this run's game directory; no prompt
    /// and no self-update.
    fn config(&self, game: &Path) -> String {
        let value = |p: &Path| p.display().to_string().replace('\\', "\\\\");
        format!(
            "hmc.java.versions={}\nhmc.java.download=false\nhmc.files.mc={}\nhmc.files.game={}\n\
             hmc.files.game-for-each-version=false\nhmc.jline.enabled=false\nhmc.updater.enabled=false\n",
            value(&self.java_home),
            value(&self.minecraft_dir),
            value(game),
        )
    }
}

/// A launched client. Stopped when dropped.
pub struct Game {
    child: Child,
    log: PathBuf,
    stopped: bool,
    /// How long the client gets to stop before it is killed.
    grace: Duration,
}

impl Game {
    /// Starts `command` in its own process group, its output in `log`.
    fn spawn(mut command: Command, log: &Path) -> Step<Self> {
        let out = fs::File::create(log).map_err(io_err("creating", log))?;
        let err = out.try_clone().map_err(io_err("opening", log))?;
        command.stdin(Stdio::null()).stdout(out).stderr(err);
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let child = command
            .spawn()
            .map_err(|e| format!("cannot start HeadlessMC: {e}"))?;
        running().push(child.id());
        Ok(Game {
            child,
            log: log.to_owned(),
            stopped: false,
            grace: STOP_GRACE,
        })
    }

    /// Why the client failed, if it did: it crashed, or exited. A crashed
    /// client does not always exit (a NeoForge client can loop on an LWJGL
    /// error after its crash report), so its crash line is the failure, and
    /// the client is stopped.
    pub fn failed(&mut self) -> Option<String> {
        let output = fs::read_to_string(&self.log).unwrap_or_default();
        if let Some(crashed) = crash(&output) {
            self.stop();
            return Some(crashed);
        }
        self.exited()
            .map(|status| format!("the client exited ({status})"))
    }

    /// The exit status, once the client has exited.
    pub fn exited(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.to_string()),
            Ok(None) => None,
            Err(e) => Some(e.to_string()),
        }
    }

    /// The last lines of HeadlessMC's output.
    pub fn tail(&self) -> String {
        fs::read_to_string(&self.log)
            .map(|text| run::last_lines(&text))
            .unwrap_or_default()
    }

    /// Stops the client. A vanilla client cannot quit by itself: SIGTERM to
    /// the process group stops HeadlessMC and the game it started, which then
    /// leaves the server as a player who quits. The game can outlive
    /// HeadlessMC: what is left of the group is killed after a grace period.
    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        let group = self.child.id();
        if !self.group_gone_within(Duration::ZERO) {
            signal_group(group, "TERM");
            if !self.group_gone_within(self.grace) {
                tracing::warn!(
                    "the client did not stop within {}s, killed",
                    self.grace.as_secs()
                );
                signal_group(group, "KILL");
                if !self.group_gone_within(KILL_WAIT) {
                    tracing::warn!("the client's process group {group} still runs after SIGKILL");
                }
            }
        }
        running().retain(|g| *g != group);
    }

    /// Whether every process of the client's group is gone within `timeout`.
    fn group_gone_within(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            // HeadlessMC, once reaped, no longer counts in its group.
            let _ = self.child.try_wait();
            if !signal_group(self.child.id(), "0") {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The crash of the game in its `output`, from the line the game prints
/// with its crash report on every loader:
/// `#@!@# Game crashed! Crash report saved to: #@!@# <path>`.
fn crash(output: &str) -> Option<String> {
    let line = output.lines().find(|l| l.contains("Game crashed!"))?;
    Some(match line.split_once("Crash report saved to:") {
        Some((_, path)) => format!(
            "the client crashed (crash report: {})",
            path.trim().trim_start_matches("#@!@#").trim()
        ),
        None => "the client crashed (no crash report saved)".into(),
    })
}

/// Sends `signal` to the process group `group`; `0` checks that the group
/// still has a process. Whether it reached one.
fn signal_group(group: u32, signal: &str) -> bool {
    Command::new("kill")
        .arg(format!("-{signal}"))
        .arg("--")
        .arg(format!("-{group}"))
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Whether `path` is the pinned HeadlessMC jar.
fn sha256_matches(path: &Path) -> Step<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    let hashes = mc_core::hash::hash_file(path).map_err(io_err("reading", path))?;
    Ok(hashes.sha256 == HEADLESSMC_SHA256)
}

/// The home of the Java at `java`, from its `java.home` property.
fn java_home(java: &Path) -> Step<PathBuf> {
    let output = Command::new(java)
        .args(["-XshowSettings:properties", "-version"])
        .output()
        .map_err(|e| format!("cannot run Java (`{}`): {e}", java.display()))?;
    parse_java_home(&String::from_utf8_lossy(&output.stderr)).ok_or_else(|| {
        format!(
            "cannot read java.home from `{} -XshowSettings:properties`",
            java.display()
        )
    })
}

fn parse_java_home(settings: &str) -> Option<PathBuf> {
    settings.lines().find_map(|line| {
        let value = line.trim().strip_prefix("java.home")?.trim_start();
        Some(PathBuf::from(value.strip_prefix('=')?.trim()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_home_is_read_from_the_settings() {
        let settings = "Property settings:\n    file.separator = /\n    java.home = /usr/lib/jvm/java-25\n    java.home.extra = no\n";
        assert_eq!(
            parse_java_home(settings),
            Some(PathBuf::from("/usr/lib/jvm/java-25"))
        );
        assert_eq!(parse_java_home("nothing"), None);
    }

    #[test]
    fn config_points_headlessmc_at_the_run() {
        let launcher = Launcher {
            java: "java".into(),
            java_home: "/jdk".into(),
            jar: "/cache/headlessmc.jar".into(),
            minecraft_dir: "/cache/minecraft".into(),
            minecraft: "26.3".into(),
        };
        let config = launcher.config(Path::new("/runs/case/client"));
        for line in [
            "hmc.java.versions=/jdk",
            "hmc.java.download=false",
            "hmc.files.mc=/cache/minecraft",
            "hmc.files.game=/runs/case/client",
            "hmc.files.game-for-each-version=false",
            "hmc.updater.enabled=false",
        ] {
            assert!(config.lines().any(|l| l == line), "{line} in {config}");
        }
    }

    /// A NeoForge client that crashed may loop on an error instead of
    /// exiting: its crash line is the failure, and the client is stopped.
    #[cfg(unix)]
    #[test]
    fn a_crashed_client_that_keeps_running_has_failed() {
        let _games = serial();
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("client.log");
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "echo 'Loading'; \
             echo '#@!@# Game crashed! Crash report saved to: #@!@# /runs/client/crash-reports/crash-client.txt'; \
             while true; do echo \"Can't return abstract class: Lorg/lwjgl/stb/STBImage;\"; sleep 0.1; done",
        ]);
        let mut game = Game::spawn(command, &log).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let failure = loop {
            if let Some(failure) = game.failed() {
                break failure;
            }
            assert!(Instant::now() < deadline, "no failure seen");
            std::thread::sleep(Duration::from_millis(50));
        };
        assert_eq!(
            failure,
            "the client crashed (crash report: /runs/client/crash-reports/crash-client.txt)"
        );
        assert!(game.exited().is_some(), "the crashed client still runs");
    }

    #[cfg(unix)]
    #[test]
    fn a_client_that_exits_without_crash_report_has_failed() {
        let _games = serial();
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("client.log");
        let mut command = Command::new("sh");
        command.args(["-c", "echo 'Process mc-client failed with code 1'; exit 1"]);
        let mut game = Game::spawn(command, &log).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let failure = loop {
            if let Some(failure) = game.failed() {
                break failure;
            }
            assert!(Instant::now() < deadline, "no failure seen");
            std::thread::sleep(Duration::from_millis(50));
        };
        assert_eq!(failure, "the client exited (exit status: 1)");
    }

    /// Serializes the tests that launch clients: [`kill_running`] kills
    /// every client of the process.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static GAMES: std::sync::Mutex<()> = std::sync::Mutex::new(());
        GAMES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether the process `pid` still runs.
    #[cfg(unix)]
    fn alive(pid: &str) -> bool {
        Command::new("kill")
            .args(["-0", pid])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    /// A client that runs `script`, which writes the pid of a process of its
    /// group to the file `pid` in `dir`, with a short grace period. Returns
    /// that pid once written.
    #[cfg(unix)]
    fn game_with_a_process(dir: &Path, script: &str) -> (Game, String) {
        let mut command = Command::new("sh");
        command.arg("-c").arg(script).current_dir(dir);
        let mut game = Game::spawn(command, &dir.join("client.log")).unwrap();
        game.grace = Duration::from_secs(1);
        let deadline = Instant::now() + Duration::from_secs(10);
        let pid = loop {
            if let Some(pid) = fs::read_to_string(dir.join("pid"))
                .ok()
                .filter(|p| p.ends_with('\n'))
            {
                break pid.trim().to_owned();
            }
            assert!(Instant::now() < deadline, "no pid written");
            std::thread::sleep(Duration::from_millis(50));
        };
        (game, pid)
    }

    /// An interrupted run kills the clients it launched, which Ctrl-C does
    /// not reach in their own process group.
    #[cfg(unix)]
    #[test]
    fn an_interrupted_run_kills_its_clients() {
        let _games = serial();
        let dir = tempfile::tempdir().unwrap();
        let (mut game, pid) = game_with_a_process(dir.path(), "sleep 60 & echo $! > pid; wait");
        kill_running();
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive(&pid) || game.exited().is_none() {
            assert!(Instant::now() < deadline, "the client still runs");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// HeadlessMC exits on SIGTERM while the game it started ignores it: the
    /// game is killed with the group.
    #[cfg(unix)]
    #[test]
    fn stopping_kills_the_game_that_outlives_headlessmc() {
        let _games = serial();
        let dir = tempfile::tempdir().unwrap();
        let (mut game, pid) = game_with_a_process(
            dir.path(),
            "sh -c 'trap \"\" TERM; while :; do sleep 0.1; done' & echo $! > pid; wait",
        );
        game.stop();
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive(&pid) {
            assert!(Instant::now() < deadline, "the game still runs");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn the_crash_line_gives_the_crash_report() {
        assert_eq!(
            crash(
                "[main/INFO]: Loading\n#@!@# Game crashed! Crash report saved to: #@!@# /r/crash.txt\n"
            ),
            Some("the client crashed (crash report: /r/crash.txt)".into())
        );
        assert_eq!(
            crash("#@!@# Game crashed! Crash report could not be saved. #@!@#"),
            Some("the client crashed (no crash report saved)".into())
        );
        assert_eq!(crash("[main/INFO]: Loading\n"), None);
    }

    #[test]
    fn vanilla_and_fabric_are_resolved_by_headlessmc() {
        let launcher = Launcher {
            java: "java".into(),
            java_home: "/jdk".into(),
            jar: "/cache/headlessmc.jar".into(),
            minecraft_dir: "/cache/minecraft".into(),
            minecraft: "26.3".into(),
        };
        let fetch = mc_core::fetch::fake::NoFetch;
        assert_eq!(
            launcher.target(&Flavour::Vanilla, &fetch).unwrap(),
            ["26.3"]
        );
        assert_eq!(
            launcher
                .target(
                    &Flavour::Fabric {
                        loader: "0.20.1".into()
                    },
                    &fetch
                )
                .unwrap(),
            ["fabric", "26.3", "0.20.1"]
        );
    }
}
