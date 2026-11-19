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
use std::time::{Duration, Instant};

use mc_core::fetch::Fetch;
use mc_core::server::loader::NEOFORGE_MAVEN;
use mc_core::server::{Step, download, io_err, run};

/// The pinned HeadlessMC release: 3.0.0 is still a release candidate, and
/// RC9 breaks compatibility with RC8.
pub const HEADLESSMC_VERSION: &str = "3.0.0-RC9";
/// SHA-256 of `headlessmc.jar` of [`HEADLESSMC_VERSION`], as GitHub lists it.
const HEADLESSMC_SHA256: &str = "a343a7c51e48c566a5cd408bd0012ddf091ea105ecc31daff00af600d3a7218a";
/// How long a stopped client gets before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(30);

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
            let partial = dir.join("headlessmc.jar.part");
            download(
                fetch,
                &format!(
                    "https://github.com/headlesshq/headlessmc/releases/download/{HEADLESSMC_VERSION}/headlessmc.jar"
                ),
                &partial,
            )?;
            if !sha256_matches(&partial)? {
                let _ = fs::remove_file(&partial);
                return Err(format!(
                    "the downloaded headlessmc.jar {HEADLESSMC_VERSION} does not have its published SHA-256"
                ));
            }
            fs::rename(&partial, &jar).map_err(io_err("writing", &jar))?;
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
                if !installed.join(format!("{id}.json")).is_file() {
                    self.install_neoforge(version, fetch)?;
                }
                vec![id]
            }
        })
    }

    fn install_neoforge(&self, version: &str, fetch: &dyn Fetch) -> Step<()> {
        let dir = &self.minecraft_dir;
        fs::create_dir_all(dir).map_err(io_err("creating", dir))?;
        // The installer wants the file of a launcher.
        let profiles = dir.join("launcher_profiles.json");
        if !profiles.is_file() {
            fs::write(&profiles, "{\"profiles\":{}}\n").map_err(io_err("writing", &profiles))?;
        }
        let installer = dir.join(format!("neoforge-{version}-installer.jar"));
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
        let _ = fs::remove_file(&installer);
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
        let out = fs::File::create(log).map_err(io_err("creating", log))?;
        let err = out.try_clone().map_err(io_err("opening", log))?;
        let mut command = Command::new(&self.java);
        command
            .arg("-jar")
            .arg(&self.jar)
            .args(["launch", "-lwjgl", "--offline", "--no-backup", "--game"])
            .arg(format!("--quickPlayMultiplayer {server}"))
            .args(target)
            .env("HMC_FILES_LOCATION", files)
            .current_dir(game)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err);
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let child = command
            .spawn()
            .map_err(|e| format!("cannot start HeadlessMC: {e}"))?;
        Ok(Game {
            child,
            log: log.to_owned(),
            stopped: false,
        })
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
}

impl Game {
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
    /// leaves the server as a player who quits; SIGKILL after a grace period.
    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        if self.exited().is_some() {
            return;
        }
        signal_group(self.child.id(), "TERM");
        let deadline = Instant::now() + STOP_GRACE;
        while Instant::now() < deadline {
            if self.exited().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        tracing::warn!(
            "the client did not stop within {}s, killed",
            STOP_GRACE.as_secs()
        );
        signal_group(self.child.id(), "KILL");
        let _ = self.child.wait();
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Sends `signal` to the process group `group`.
fn signal_group(group: u32, signal: &str) {
    let _ = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg("--")
        .arg(format!("-{group}"))
        .status();
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
