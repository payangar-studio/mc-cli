//! `mod port <version>`: moves the mod to another Minecraft version, on a new
//! branch named after it, with the versions the socle pins for it. The code
//! that breaks is fixed afterwards, with the socle's `mod-port` skill.

use std::path::Path;
use std::process::Command;

use crate::context::Context;
use crate::error::{Context as _, Error, Result};
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
    print!("{}", summary(&args.version, &ported));
    Ok(())
}

/// What [`port`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ported {
    /// The branch the new one starts from.
    from_branch: String,
    from: Versions,
    to: Versions,
}

/// Ports the mod at `root` to Minecraft `target`, whose versions come from
/// `table`: writes the versions pinned for `target` in the socle settings,
/// then creates the branch `target` from the current commit and switches to
/// it. Touches no other file and commits nothing.
///
/// The socle must be this CLI's: the versions of `table` go with the build
/// it lays down. Branches are looked up locally and among the remote-tracking
/// branches, without fetching.
fn port(root: &Path, target: &str, table: &[Pinned]) -> Result<Ported> {
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
    if config.version != SOCLE.version {
        return Err(Error::usage(
            format!(
                "this repository uses socle {}, but the versions this mc-cli pins go with the build of socle {}",
                config.version, SOCLE.version
            ),
            format!("run `mod update`, commit, then rerun `mod port {target}`"),
        ));
    }
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
    if git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{target}"),
        ],
    )
    .is_ok()
    {
        return Err(Error::usage(
            format!("branch {target} already exists"),
            format!("the mod is already ported: `git switch {target}`"),
        ));
    }
    let remote = git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            &format!("refs/remotes/*/{target}"),
        ],
    )?;
    if let Some(remote) = remote.lines().next() {
        let name = remote.strip_suffix(&format!("/{target}")).unwrap_or(remote);
        return Err(Error::usage(
            format!("branch {target} already exists on {name} ({remote})"),
            format!("the mod is already ported: `git switch {target}`"),
        ));
    }

    // The settings are written first: `git switch --create` carries them to
    // the new branch, and a failed write leaves no branch behind.
    let path = root.join(SOCLE.config_path());
    let original = std::fs::read(&path).context(format!("reading {}", path.display()))?;
    let from = std::mem::replace(&mut config.init, pinned.versions());
    config.save(&SOCLE, root)?;
    if let Err(err) = git_or(root, &["switch", "--quiet", "--create", target], |what| {
        Error::internal(
            what,
            format!(
                "{} is left as it was; fix the error above, then rerun `mod port {target}`",
                SOCLE.config_path()
            ),
        )
    }) {
        std::fs::write(&path, original).context(format!("restoring {}", path.display()))?;
        return Err(err);
    }
    Ok(Ported {
        from_branch,
        from,
        to: config.init,
    })
}

/// What `mod port` prints once the mod is ported to `target`.
fn summary(target: &str, ported: &Ported) -> String {
    let mut out = format!(
        "Created branch {target} from {}, with these versions in {}:\n",
        ported.from_branch,
        SOCLE.config_path()
    );
    for (name, from, to) in changes(&ported.from, &ported.to) {
        out.push_str(&format!("  {name}: {from} -> {to}\n"));
    }
    out.push_str(
        "Nothing is committed. Build with `./gradlew build`, then follow the `mod-port` skill to fix what breaks.\n",
    );
    out
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
        ("modmenu", &from.modmenu, &to.modmenu),
    ]
    .into_iter()
    .filter(|(_, a, b)| a != b)
    .map(|(name, a, b)| (name, a.as_str(), b.as_str()))
    .collect()
}

/// Runs git in `root` for a check made before any change: a failure means the
/// mod is not a git repository `mod port` can work in.
fn git(root: &Path, args: &[&str]) -> Result<String> {
    git_or(root, args, |what| {
        Error::usage(
            what,
            "the mod must be a git repository with at least one commit; fix the error above, then rerun `mod port`",
        )
    })
}

/// Runs git in `root`: its stdout without the trailing newline, or the error
/// `failed` makes of what went wrong.
fn git_or(root: &Path, args: &[&str], failed: impl FnOnce(String) -> Error) -> Result<String> {
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
        return Err(failed(format!(
            "`{command}` failed in {}: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    /// A version table with the version the fixture mod is on and a later one.
    const TABLE: &[Pinned] = &[
        Pinned {
            minecraft: "26.4",
            fabric_loader: "0.21.0",
            fabric_api: "0.170.0+26.4",
            loom: "1.20.1",
            neoforge: "26.4.0.7",
            moddevgradle: "2.0.150",
            modmenu: "21.1.0",
        },
        Pinned {
            minecraft: "26.3",
            fabric_loader: "0.20.1",
            fabric_api: "0.166.2+26.3",
            loom: "1.19.4",
            neoforge: "26.3.0.113",
            moddevgradle: "2.0.148",
            modmenu: "21.0.1",
        },
    ];

    /// Runs git in `dir`, which must succeed; its stdout without the
    /// trailing newline.
    fn sh_git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
    }

    /// A repository on branch `26.3` holding only the socle settings of this
    /// CLI's socle on Minecraft 26.3, committed: all `port` reads.
    fn mod_on_26_3() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        sh_git(root, &["init", "-q", "-b", "26.3"]);
        SocleConfig {
            version: SOCLE.version.to_owned(),
            init: TABLE[1].versions(),
        }
        .save(&SOCLE, root)
        .unwrap();
        sh_git(root, &["add", "-A"]);
        sh_git(root, &["commit", "-q", "-m", "init"]);
        dir
    }

    fn config_text(root: &Path) -> String {
        std::fs::read_to_string(root.join(SOCLE.config_path())).unwrap()
    }

    #[test]
    fn port_creates_the_version_branch_with_its_versions() {
        let dir = mod_on_26_3();
        let root = dir.path();
        let head = sh_git(root, &["rev-parse", "HEAD"]);

        let ported = port(root, "26.4", TABLE).unwrap();

        assert_eq!(ported.from_branch, "26.3");
        assert_eq!(ported.from, TABLE[1].versions());
        assert_eq!(ported.to, TABLE[0].versions());
        // The new branch starts from the old one, which is left as it was.
        assert_eq!(sh_git(root, &["branch", "--show-current"]), "26.4");
        assert_eq!(sh_git(root, &["rev-parse", "HEAD"]), head);
        assert_eq!(sh_git(root, &["rev-parse", "26.3"]), head);
        // Only the versions of the socle settings change.
        assert_eq!(
            sh_git(root, &["status", "--porcelain"]),
            " M .mod/socle.toml"
        );
        let after = SocleConfig::load(&SOCLE, root).unwrap();
        assert_eq!(after.version, SOCLE.version);
        assert_eq!(after.init, TABLE[0].versions());
        let text = config_text(root);
        assert!(text.contains("minecraft = \"26.4\""), "{text}");
        assert!(text.contains("fabric-api = \"0.170.0+26.4\""), "{text}");
    }

    /// The settings are written before the branch is created: a failed write
    /// leaves no branch behind that would make a rerun say "already ported".
    #[test]
    fn a_failed_write_creates_no_branch() {
        let dir = mod_on_26_3();
        let root = dir.path();
        // The settings are written through `socle.toml.tmp`; an empty
        // directory there makes the write fail and is invisible to git.
        std::fs::create_dir(root.join(".mod/socle.toml.tmp")).unwrap();

        port(root, "26.4", TABLE).unwrap_err();

        assert_eq!(sh_git(root, &["branch", "--list"]), "* 26.3");
        assert_eq!(sh_git(root, &["status", "--porcelain"]), "");
    }

    /// A branch that git cannot create is not a usage error, and leaves the
    /// settings as they were.
    #[test]
    fn a_failed_branch_creation_restores_the_settings() {
        let dir = mod_on_26_3();
        let root = dir.path();
        let before = config_text(root);
        // `26.4/fix` keeps git from creating `26.4`.
        sh_git(root, &["branch", "26.4/fix"]);

        let err = port(root, "26.4", TABLE).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::Internal, "{err}");
        assert!(err.to_string().contains("git switch"), "{err}");
        assert!(!err.hint().contains("git repository"), "{err}");
        assert_eq!(sh_git(root, &["branch", "--show-current"]), "26.3");
        assert_eq!(config_text(root), before);
        assert_eq!(sh_git(root, &["status", "--porcelain"]), "");
    }

    /// A branch of the version fetched from any remote means the mod is
    /// already ported.
    #[test]
    fn a_remote_tracking_branch_is_refused() {
        let dir = mod_on_26_3();
        let root = dir.path();
        sh_git(root, &["update-ref", "refs/remotes/upstream/26.4", "HEAD"]);

        let err = port(root, "26.4", TABLE).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::Usage);
        assert!(
            err.to_string()
                .contains("branch 26.4 already exists on upstream"),
            "{err}"
        );
        assert_eq!(sh_git(root, &["branch", "--list"]), "* 26.3");
    }

    #[test]
    fn the_summary_names_both_branches_and_each_changed_version() {
        let dir = mod_on_26_3();
        let ported = port(dir.path(), "26.4", TABLE).unwrap();

        assert_eq!(
            summary("26.4", &ported),
            "Created branch 26.4 from 26.3, with these versions in .mod/socle.toml:\n\
             \x20 minecraft: 26.3 -> 26.4\n\
             \x20 fabric-loader: 0.20.1 -> 0.21.0\n\
             \x20 fabric-api: 0.166.2+26.3 -> 0.170.0+26.4\n\
             \x20 loom: 1.19.4 -> 1.20.1\n\
             \x20 neoforge: 26.3.0.113 -> 26.4.0.7\n\
             \x20 moddevgradle: 2.0.148 -> 2.0.150\n\
             \x20 modmenu: 21.0.1 -> 21.1.0\n\
             Nothing is committed. Build with `./gradlew build`, then follow the `mod-port` skill to fix what breaks.\n"
        );
    }

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
