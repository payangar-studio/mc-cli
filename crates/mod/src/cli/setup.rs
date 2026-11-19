//! `mod setup`: configures the GitHub repository of the mod, once it exists:
//! the ruleset of the release tags, the vault variables, `CODEOWNERS`
//! ([`crate::github`]).

use std::path::Path;
use std::process::Command;

use crate::context::Context;
use crate::error::{Error, Result};
use crate::github::{self, GitHub, Repo};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Leave the GitHub repository alone
    #[arg(long)]
    pub no_github: bool,
    /// URL of the vault, set as the repository variable INFISICAL_URL
    #[arg(long)]
    pub vault_url: Option<String>,
    /// Id of the CI's vault identity, set as the repository variable INFISICAL_IDENTITY_ID
    #[arg(long)]
    pub identity_id: Option<String>,
}

pub fn run(args: Args, ctx: &Context) -> Result<()> {
    let root = ctx.require_root()?;
    if args.no_github {
        println!("Skipped the GitHub repository (--no-github).");
        return Ok(());
    }
    let Some(url) = origin(root) else {
        println!(
            "No GitHub remote `origin`: skipped the GitHub repository. Push the mod to GitHub, then run `mod setup` again."
        );
        return Ok(());
    };
    let Some(repo) = Repo::from_remote(&url) else {
        println!(
            "The remote `origin` ({}) is not a github.com URL: skipped the GitHub repository. Set it to https://github.com/<owner>/<repository>, then run `mod setup` again.",
            url.trim()
        );
        return Ok(());
    };
    let gh = GitHub::new(github::API, token()?);
    let done = github::configure(
        &gh,
        &repo,
        root,
        [args.vault_url, args.identity_id],
        |name, question| {
            let arg = match name {
                "INFISICAL_URL" => "--vault-url",
                _ => "--identity-id",
            };
            ctx.prompt.text(None, arg, question)
        },
    )?;
    println!("GitHub repository {repo}:");
    for line in done {
        println!("  {line}");
    }
    Ok(())
}

/// The URL of the remote `origin`, if any.
fn origin(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// A GitHub token: `GH_TOKEN`, `GITHUB_TOKEN`, else the one of the `gh` CLI.
fn token() -> Result<String> {
    for var in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Some(token) = std::env::var(var).ok().filter(|t| !t.trim().is_empty()) {
            return Ok(token.trim().to_owned());
        }
    }
    Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            Error::usage(
                "no GitHub token: GH_TOKEN and GITHUB_TOKEN are unset, and `gh auth token` gave none",
                "run `gh auth login`, or set GH_TOKEN; pass --no-github to skip GitHub",
            )
        })
}
