//! `mod port <version>`: moves the mod to another Minecraft version, on a new
//! branch named after it, with the versions the socle pins for it. The code
//! that breaks is fixed afterwards, with the socle's `mod-port` skill.

use std::path::Path;
use std::process::Command;

use crate::context::Context;
use crate::error::{Error, Result};
use crate::model::socle::{SocleConfig, Versions};
use crate::socle::SOCLE;
use crate::versions::{self, Pinned};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Minecraft version to port the mod to, which names the new branch
    pub version: String,
}

pub fn run(args: Args, ctx: &Context) -> Result<()> {
    let root = ctx.require_root()?;
    let ported = port(root, &args.version, versions::SUPPORTED)?;
    println!(
        "Created branch {} from {}, with these versions in {}:",
        args.version,
        ported.from_branch,
        SOCLE.config_path()
    );
    for (name, from, to) in changes(&ported.from, &ported.to) {
        println!("  {name}: {from} -> {to}");
    }
    println!(
        "Nothing is committed. Build with `./gradlew build`, then follow the `mod-port` skill to fix what breaks."
    );
    Ok(())
}

/// What [`port`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ported {
    /// The branch the new one starts from.
    pub from_branch: String,
    pub from: Versions,
    pub to: Versions,
}

/// Ports the mod at `root` to Minecraft `target`, whose versions come from
/// `table`: creates the branch `target` from the current commit, switches to
/// it, and writes the versions pinned for `target` in the socle settings.
/// Touches no other file and commits nothing.
pub fn port(root: &Path, target: &str, table: &[Pinned]) -> Result<Ported> {
    let pinned = table.iter().find(|p| p.minecraft == target).ok_or_else(|| {
        let supported: Vec<&str> = table.iter().map(|p| p.minecraft).collect();
        Error::usage(
            format!(
                "Minecraft {target} is not supported by this mc-cli ({}), which supports: {}",
                SOCLE.version,
                supported.join(", ")
            ),
            format!(
                "update mc-cli to a release that supports Minecraft {target}, then rerun `mod port {target}`"
            ),
        )
    })?;
    let mut config = SocleConfig::load(&SOCLE, root)?;
    if config.init.minecraft == target {
        return Err(Error::usage(
            format!("the mod is already on Minecraft {target}"),
            "pass another Minecraft version",
        ));
    }

    let from_branch = git(root, &["branch", "--show-current"])?;
    let from_branch = if from_branch.is_empty() {
        git(root, &["rev-parse", "--short", "HEAD"])?
    } else {
        from_branch
    };
    let changed = git(root, &["status", "--porcelain"])?;
    if !changed.is_empty() {
        return Err(Error::usage(
            format!("the working tree has uncommitted changes:\n{changed}"),
            format!(
                "commit or stash them on {from_branch}, then rerun `mod port {target}`: they would move to the new branch"
            ),
        ));
    }
    for (git_ref, hint) in [
        (
            format!("refs/heads/{target}"),
            format!("the mod is already ported: `git switch {target}`"),
        ),
        (
            format!("refs/remotes/origin/{target}"),
            format!("the mod is already ported on origin: `git switch {target}`"),
        ),
    ] {
        if git(root, &["rev-parse", "--verify", "--quiet", &git_ref]).is_ok() {
            return Err(Error::usage(
                format!("branch {target} already exists"),
                hint,
            ));
        }
    }

    git(root, &["switch", "--quiet", "--create", target])?;
    let from = std::mem::replace(&mut config.init, pinned.versions());
    config.save(&SOCLE, root)?;
    Ok(Ported {
        from_branch,
        from,
        to: config.init,
    })
}

/// The versions that differ, as (name, from, to).
fn changes<'a>(from: &'a Versions, to: &'a Versions) -> Vec<(&'static str, &'a str, &'a str)> {
    [
        ("minecraft", &from.minecraft, &to.minecraft),
        ("fabric-loader", &from.fabric_loader, &to.fabric_loader),
        ("fabric-api", &from.fabric_api, &to.fabric_api),
        ("loom", &from.loom, &to.loom),
        ("neoforge", &from.neoforge, &to.neoforge),
        ("moddevgradle", &from.moddevgradle, &to.moddevgradle),
    ]
    .into_iter()
    .filter(|(_, a, b)| a != b)
    .map(|(name, a, b)| (name, a.as_str(), b.as_str()))
    .collect()
}

/// Runs git in `root`; its trimmed stdout, or an error naming the command.
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let command = format!("git {}", args.join(" "));
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| {
            Error::internal(
                format!("could not run `{command}`"),
                "install git, then rerun `mod port`",
            )
            .with_source(e)
        })?;
    if !output.status.success() {
        return Err(Error::usage(
            format!(
                "`{command}` failed in {}: {}",
                root.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            "the mod must be a git repository with at least one commit; fix the error above, then rerun `mod port`",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_versions_are_listed() {
        let from = versions::SUPPORTED[0].versions();
        let mut to = from.clone();
        to.minecraft = "26.4".into();
        to.loom = "1.20.0".into();
        assert_eq!(
            changes(&from, &to),
            [
                ("minecraft", "26.3", "26.4"),
                ("loom", from.loom.as_str(), "1.20.0")
            ]
        );
    }
}
