//! `server-starts`: a dedicated server with the variant's mods reaches `Done`.
//!
//! The server of the pack's loader and Minecraft version comes from the
//! shared server code ([`mc_core::server`]), installed once into the
//! machine's cache. Each check copies it into a temporary directory, adds the
//! variant's jars but the client-only ones (`env.server = "unsupported"`), the
//! base config then the variant's overrides, accepts the EULA, and starts the
//! server without its interface until the `Done` line of its log.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use mc_core::server::loader::{self, Loader};
use mc_core::server::{
    Step, copy_tree, installed, io_err, java, link_or_copy, ready_to_start, reason, run, run_dir,
};

use super::Status;
use crate::model::manifest::{Kind, Manifest, Support};
use crate::model::variants::Resolved;
use crate::store;

pub use mc_core::server::Machine;

pub fn server_starts(
    root: &Path,
    manifest: &Manifest,
    variant: &Resolved,
    timeout: Duration,
    machine: &Machine<'_>,
) -> Status {
    let pack = &manifest.pack;
    let Some(loader) = Loader::of(&pack.loader) else {
        return Status::Skipped(format!(
            "no scriptable server install for the loader `{}` (handled: {})",
            pack.loader,
            loader::SUPPORTED
        ));
    };
    let have = match java::major_version(&machine.java) {
        Ok(Some(have)) => have,
        Ok(None) => {
            return Status::Skipped(format!(
                "no Java found (`{}`); set JAVA_HOME or put java on the PATH to run this check",
                machine.java.display()
            ));
        }
        Err(reason) => return Status::Failed(reason),
    };
    let start = || -> Step<()> {
        let server = installed(loader, pack, have, machine)?;
        let run_dir = prepare(root, manifest, variant, &server, machine)?;
        let mut command = Command::new(&machine.java);
        command
            .args(loader.launch_args(pack))
            .current_dir(run_dir.path());
        tracing::info!(variant = %variant.name, dir = %run_dir.path().display(), "starting the server");
        run::until_done(command, timeout)
    };
    match start() {
        Ok(()) => Status::Passed,
        Err(reason) => Status::Failed(reason),
    }
}

/// A temporary server directory for the variant, removed when dropped.
fn prepare(
    root: &Path,
    manifest: &Manifest,
    variant: &Resolved,
    server: &Path,
    machine: &Machine<'_>,
) -> Step<tempfile::TempDir> {
    let run = run_dir(server, &variant.name, machine)?;
    let dir = run.path();

    let mods = dir.join(Kind::Mod.dir());
    fs::create_dir_all(&mods).map_err(io_err("creating", &mods))?;
    let mut missing = Vec::new();
    let mut local_missing = Vec::new();
    // A client-only mod is left out, as a server installed from the .mrpack
    // leaves it out; so is every resource pack, which a server never loads.
    let on_server = variant
        .keys
        .iter()
        .filter_map(|k| manifest.get(k))
        .filter(|e| e.kind == Kind::Mod && e.env.server != Support::Unsupported);
    for entry in on_server {
        let jar = store::file_path(root, entry).map_err(reason)?;
        if !jar.is_file() {
            match entry.is_local() {
                true => local_missing.push(entry.path.as_str()),
                false => missing.push(entry.key.as_str()),
            }
            continue;
        }
        let name = jar.file_name().expect("file_path ends with a file name");
        link_or_copy(&jar, &mods.join(name))?;
    }
    if !missing.is_empty() {
        return Err(format!(
            "jars missing from mods/: {}; run `modpack install`",
            missing.join(", ")
        ));
    }
    // Git holds the jars of mods-local/: `modpack install` cannot restore
    // them.
    if !local_missing.is_empty() {
        return Err(format!(
            "local jars missing from mods-local/: {}; restore them from git with `git checkout -- {}`",
            local_missing.join(", "),
            local_missing.join(" ")
        ));
    }

    for config in [
        root.join("config"),
        root.join("overrides").join(&variant.name).join("config"),
    ] {
        if config.is_dir() {
            copy_tree(&config, &dir.join("config"), false)?;
        }
    }
    ready_to_start(dir)?;
    Ok(run)
}

#[cfg(test)]
pub(super) mod tests;
