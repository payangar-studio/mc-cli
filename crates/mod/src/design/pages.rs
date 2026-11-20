//! The git side of publishing the `design` branch, run by `mod setup` once the
//! GitHub repository exists ([`crate::github::configure`]): which branches
//! `origin` has, and the push of `design`. GitHub Pages then serves the page
//! from `/docs` at `https://<owner>.github.io/<repo>/design/`.
//!
//! On an empty GitHub repository, the first branch pushed becomes the default
//! one. The default branch must be the newest Minecraft version, so `design`
//! is pushed only once that version branch is on `origin`.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

use super::BRANCH;

/// The folder of the branch Pages publishes.
pub const FOLDER: &str = "/docs";

/// The branches of the mod on `origin`, read before setup changes anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    /// The newest Minecraft version branch, here or on `origin`, which is on
    /// `origin`: the default branch.
    pub version: String,
    pub design: Design,
}

/// Where the `design` branch is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Design {
    OnRemote,
    /// Here only: setup pushes it.
    Local,
    /// Neither here nor on `origin`.
    Missing,
}

/// Reads the branches of `root` and of its `origin`. Refuses when the newest
/// Minecraft version branch is not on `origin` yet.
pub fn read(root: &Path) -> Result<Remote> {
    let local = git(root, &["for-each-ref", "--format=%(refname)", "refs/heads"])?;
    let local: Vec<&str> = local
        .lines()
        .filter_map(|r| r.strip_prefix("refs/heads/"))
        .collect();
    let remote = git(root, &["ls-remote", "--heads", "origin"])?;
    let remote: Vec<&str> = remote
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter_map(|(_, r)| r.strip_prefix("refs/heads/"))
        .collect();
    let Some(version) = local
        .iter()
        .chain(&remote)
        .filter_map(|b| version_key(b).map(|key| (key, *b)))
        .max()
        .map(|(_, b)| b.to_owned())
    else {
        return Err(Error::usage(
            "no branch named after a Minecraft version, here or on origin",
            "a mod lives on a branch named after its Minecraft version (`26.3`): create it, push it, then run `mod setup` again",
        ));
    };
    if !remote.contains(&version.as_str()) {
        return Err(Error::usage(
            format!(
                "the branch {version} is not on origin yet: it must be the default branch of the GitHub repository, and an empty repository makes the first branch pushed the default"
            ),
            format!("push it with `git push -u origin {version}`, then run `mod setup` again"),
        ));
    }
    let design = if remote.contains(&BRANCH) {
        Design::OnRemote
    } else if local.contains(&BRANCH) {
        Design::Local
    } else {
        Design::Missing
    };
    Ok(Remote { version, design })
}

/// Pushes the local `design` branch to `origin`.
pub fn push(root: &Path) -> Result<()> {
    let refspec = format!("refs/heads/{BRANCH}:refs/heads/{BRANCH}");
    let output = run(root, &["push", "--quiet", "origin", &refspec])?;
    if !output.status.success() {
        return Err(Error::internal(
            format!(
                "could not push the `{BRANCH}` branch to origin: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            format!("push it with `git push origin {BRANCH}`, then run `mod setup` again"),
        ));
    }
    Ok(())
}

/// The numbers of a Minecraft version branch (`26.3`, `1.21.10`), which
/// order the versions; `None` for any other branch.
fn version_key(branch: &str) -> Option<Vec<u64>> {
    let parts: Vec<u64> = branch
        .split('.')
        .map(|p| {
            (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
                .then(|| p.parse().ok())
                .flatten()
        })
        .collect::<Option<_>>()?;
    (parts.len() >= 2).then_some(parts)
}

/// Runs git in `root`; returns its trimmed stdout, or an error naming what
/// git said.
fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = run(root, args)?;
    if !output.status.success() {
        return Err(Error::internal(
            format!(
                "`git {}` failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            "check that origin is reachable with `git ls-remote origin`, then run `mod setup` again",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn run(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| Error::internal("could not run git", "install git").with_source(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_branches_are_ordered_by_their_numbers() {
        let mut branches = vec!["26.10", "1.21.10", "26.3", "26.9", "26.3.1"];
        branches.sort_by_key(|b| version_key(b));
        assert_eq!(branches, ["1.21.10", "26.3", "26.3.1", "26.9", "26.10"]);
        for other in ["design", "main", "26", "26.4/fix", "26.x", "26..3", "v26.3"] {
            assert_eq!(version_key(other), None, "{other}");
        }
    }
}
