//! The dedicated server: installed once per loader and versions into the
//! machine's cache, with the loader's official installer, then copied into a
//! temporary directory for each run (hard links when the filesystem allows),
//! and started without its interface until the `Done` line of its log.
//!
//! What goes into a run (mods, config) is the caller's: it fills the run
//! directory between [`run_dir`] and [`ready_to_start`].

pub mod java;
pub mod loader;
pub mod run;

use std::fs;
use std::io;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::error::{Error, Result};
use crate::fetch::Fetch;
use crate::model::entry::Pack;
use loader::Loader;

/// A step of a server check: its error is the reason of the failure.
pub type Step<T> = std::result::Result<T, String>;

/// File of an installed server holding the Java major version it needs. It is
/// written last: a server without it is an unfinished install.
pub const READY: &str = ".modpack-java";

/// What running a server needs from the machine.
pub struct Machine<'a> {
    pub fetch: &'a dyn Fetch,
    /// The Java that runs installers and servers.
    pub java: PathBuf,
    /// Where installed servers are kept between runs.
    pub cache_dir: PathBuf,
}

impl<'a> Machine<'a> {
    /// `$JAVA_HOME/bin/java`, else `java` from the PATH; servers cached in
    /// `<user cache dir>/<app>/servers`.
    pub fn from_env(fetch: &'a dyn Fetch, app: &str) -> Result<Self> {
        let java = match std::env::var_os("JAVA_HOME").filter(|v| !v.is_empty()) {
            Some(home) => PathBuf::from(home).join("bin").join("java"),
            None => PathBuf::from("java"),
        };
        let dirs = directories::BaseDirs::new().ok_or_else(|| {
            Error::internal(
                "cannot find the user's home directory",
                "set HOME to the user's home directory",
            )
        })?;
        Ok(Self {
            fetch,
            java,
            cache_dir: dirs.cache_dir().join(app).join("servers"),
        })
    }
}

/// The cached server of `pack`, installed first if needed, for the Java
/// whose major version is `java`.
pub fn installed(loader: Loader, pack: &Pack, java: u32, machine: &Machine<'_>) -> Step<PathBuf> {
    let dir = machine.cache_dir.join(safe_name(&format!(
        "{}-{}-{}",
        loader.name(),
        pack.minecraft,
        pack.loader_version
    )));
    let ready = fs::read_to_string(dir.join(READY))
        .ok()
        .and_then(|t| t.trim().parse::<u32>().ok());
    if let Some(required) = ready {
        java::check(java, required, &pack.minecraft, &machine.java)?;
        return Ok(dir);
    }

    let required = java::required(&pack.minecraft, machine.fetch)?;
    java::check(java, required, &pack.minecraft, &machine.java)?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(io_err("removing", &dir))?;
    }
    fs::create_dir_all(&machine.cache_dir).map_err(io_err("creating", &machine.cache_dir))?;
    let staging = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(&machine.cache_dir)
        .map_err(io_err("creating a directory in", &machine.cache_dir))?;
    loader.install(pack, staging.path(), &machine.java, machine.fetch)?;
    fs::write(staging.path().join(READY), required.to_string())
        .map_err(io_err("writing in", staging.path()))?;
    let staging = staging.keep();
    if let Err(e) = fs::rename(&staging, &dir) {
        let _ = fs::remove_dir_all(&staging);
        // Another check may have installed the same server meanwhile.
        if !dir.join(READY).is_file() {
            return Err(format!("cannot write {}: {e}", dir.display()));
        }
    }
    Ok(dir)
}

/// A temporary directory for a run of `server`, holding a copy of it, removed
/// when dropped. `name` names it among the runs.
pub fn run_dir(server: &Path, name: &str, machine: &Machine<'_>) -> Step<tempfile::TempDir> {
    let runs = machine.cache_dir.join(".runs");
    fs::create_dir_all(&runs).map_err(io_err("creating", &runs))?;
    let run = tempfile::Builder::new()
        .prefix(&format!("{}-", safe_name(name)))
        .tempdir_in(&runs)
        .map_err(io_err("creating a directory in", &runs))?;
    copy_tree(server, run.path(), true)?;
    Ok(run)
}

/// Accepts the EULA in the run directory `dir`, and gives the server a free
/// port, so that a server already running on this machine does not make the
/// run fail.
pub fn ready_to_start(dir: &Path) -> Step<()> {
    write(&dir.join("eula.txt"), "eula=true\n")?;
    let port = free_port()?;
    write(
        &dir.join("server.properties"),
        &format!("server-port={port}\n"),
    )
}

/// A TCP port free on this machine right now.
pub fn free_port() -> Step<u16> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| format!("cannot find a free port: {e}"))
}

/// Copies `from` into `to`, replacing files; with `link`, hard-links files
/// when the filesystem allows. The server only reads what is linked.
pub fn copy_tree(from: &Path, to: &Path, link: bool) -> Step<()> {
    fs::create_dir_all(to).map_err(io_err("creating", to))?;
    for entry in fs::read_dir(from).map_err(io_err("reading", from))? {
        let entry = entry.map_err(io_err("reading", from))?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if entry
            .file_type()
            .map_err(io_err("reading", &source))?
            .is_dir()
        {
            copy_tree(&source, &target, link)?;
        } else if link {
            link_or_copy(&source, &target)?;
        } else {
            fs::copy(&source, &target).map_err(io_err("copying", &source))?;
        }
    }
    Ok(())
}

/// Hard-links `from` to `to` when the filesystem allows, else copies it.
pub fn link_or_copy(from: &Path, to: &Path) -> Step<()> {
    if fs::hard_link(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)
        .map(|_| ())
        .map_err(io_err("copying", from))
}

fn write(path: &Path, text: &str) -> Step<()> {
    fs::write(path, text).map_err(io_err("writing", path))
}

/// Downloads `url` into the file `to`.
pub fn download(fetch: &dyn Fetch, url: &str, to: &Path) -> Step<()> {
    let mut body = fetch.get(url).map_err(reason)?;
    let mut file = fs::File::create(to).map_err(io_err("creating", to))?;
    io::copy(&mut body, &mut file).map_err(|e| format!("cannot download {url}: {e}"))?;
    Ok(())
}

fn get_json<T: DeserializeOwned>(fetch: &dyn Fetch, url: &str) -> Step<T> {
    serde_json::from_reader(fetch.get(url).map_err(reason)?)
        .map_err(|e| format!("unexpected answer from {url}: {e}"))
}

/// An error of the CLI as the reason of a failure.
pub fn reason(err: Error) -> String {
    let text = err.to_string();
    text.strip_prefix("error: ").unwrap_or(&text).to_owned()
}

/// The reason of a failed file operation: what was done, on which path, and
/// the error.
pub fn io_err(doing: &'static str, path: &Path) -> impl FnOnce(io::Error) -> String {
    let path = path.display().to_string();
    move |e| format!("{doing} {path}: {e}")
}

/// A name usable as a directory name on every system.
pub fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_made_safe_for_directories() {
        assert_eq!(
            safe_name("neoforge-1.21.1-21.1.77"),
            "neoforge-1.21.1-21.1.77"
        );
        assert_eq!(safe_name("a/../b c"), "a_.._b_c");
    }
}
