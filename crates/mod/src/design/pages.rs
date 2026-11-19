//! GitHub Pages of the `design` branch: once the mod's GitHub repository
//! exists (the remote `origin`), the branch is pushed there and Pages is set
//! to publish it from `/docs`, through the REST API called by the `gh` CLI.
//! The page is then served at `https://<owner>.github.io/<repo>/design/`.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

use super::BRANCH;

/// The folder of the branch Pages publishes.
pub const FOLDER: &str = "/docs";

/// What [`publish`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Published {
    /// Pages publishes the branch; the URL of the design page.
    Configured { url: String },
    /// `origin` is not a GitHub repository, or there is no `origin`.
    NoGitHubRemote,
    /// `origin` names a GitHub repository git cannot reach (not created yet,
    /// or no access); what git said.
    Unreachable { repo: Repo, reason: String },
}

/// A GitHub repository, as `owner/name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
}

impl std::fmt::Display for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

impl Repo {
    /// The repository a git remote URL points to, when it is on github.com:
    /// `https://github.com/o/r(.git)`, `git@github.com:o/r.git`,
    /// `ssh://git@github.com/o/r.git`.
    pub fn from_remote(url: &str) -> Option<Self> {
        let rest = [
            "https://github.com/",
            "git@github.com:",
            "ssh://git@github.com/",
        ]
        .iter()
        .find_map(|prefix| url.trim().strip_prefix(prefix))?;
        let rest = rest.trim_end_matches('/');
        let rest = rest.strip_suffix(".git").unwrap_or(rest);
        let (owner, name) = rest.split_once('/')?;
        let valid = |s: &str| !s.is_empty() && !s.contains('/');
        (valid(owner) && valid(name)).then(|| Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }
}

/// What a `gh` run gave.
#[derive(Debug, Clone, Default)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Pushes the `design` branch of `root` to its GitHub `origin` when missing
/// there, then sets Pages to publish it from `/docs`.
pub fn publish(root: &Path) -> Result<Published> {
    let Some(repo) = git(root, &["config", "--get", "remote.origin.url"])
        .filter(|o| o.success)
        .and_then(|o| Repo::from_remote(&o.stdout))
    else {
        return Ok(Published::NoGitHubRemote);
    };
    let heads = git(root, &["ls-remote", "--heads", "origin"])
        .ok_or_else(|| Error::internal("could not run git", "install git"))?;
    if !heads.success {
        return Ok(Published::Unreachable {
            repo,
            reason: heads.stderr,
        });
    }
    let branch = format!("refs/heads/{BRANCH}");
    if !heads
        .stdout
        .lines()
        .any(|l| l.ends_with(&format!("\t{branch}")))
    {
        let refspec = format!("refs/heads/{BRANCH}:refs/heads/{BRANCH}");
        let push = git(root, &["push", "--quiet", "origin", &refspec])
            .ok_or_else(|| Error::internal("could not run git", "install git"))?;
        if !push.success {
            return Err(Error::internal(
                format!(
                    "could not push the `{BRANCH}` branch to {repo}: {}",
                    push.stderr.trim()
                ),
                format!("push it with `git push origin {BRANCH}`, then set up GitHub Pages"),
            ));
        }
    }
    let url = configure(&repo, |args| run_gh(root, args))?;
    Ok(Published::Configured { url })
}

/// Sets Pages of `repo` to publish `design` from `/docs`, creating the site
/// when there is none and changing its source when it differs. `gh` runs
/// `gh` with the given arguments. Returns the URL of the design page.
pub fn configure(repo: &Repo, gh: impl Fn(&[&str]) -> Result<Output>) -> Result<String> {
    let path = format!("repos/{repo}/pages");
    let source = [
        "-f",
        "build_type=legacy",
        "-f",
        &format!("source[branch]={BRANCH}"),
        "-f",
        &format!("source[path]={FOLDER}"),
    ]
    .map(String::from);
    let source: Vec<&str> = source.iter().map(String::as_str).collect();
    let jq = ".build_type + \" \" + .source.branch + \" \" + .source.path + \" \" + .html_url";

    let current = gh(&["api", &path, "--jq", jq])?;
    let site = if current.success {
        current.stdout.trim().to_owned()
    } else if current.stderr.contains("HTTP 404") {
        let mut args = vec!["api", "--method", "POST", &path];
        args.extend(&source);
        args.extend(["--jq", jq]);
        let created = gh(&args)?;
        if !created.success {
            return Err(failed("create", repo, &created));
        }
        created.stdout.trim().to_owned()
    } else {
        return Err(failed("read", repo, &current));
    };

    let fields: Vec<&str> = site.split(' ').collect();
    let [build_type, branch, folder, html_url] = fields[..] else {
        return Err(Error::internal(
            format!("unexpected answer of GitHub about the Pages site of {repo}: `{site}`"),
            "check the Pages settings of the repository on GitHub",
        ));
    };
    if (build_type, branch, folder) != ("legacy", BRANCH, FOLDER) {
        let mut args = vec!["api", "--method", "PUT", &path];
        args.extend(&source);
        let updated = gh(&args)?;
        if !updated.success {
            return Err(failed("update", repo, &updated));
        }
    }
    Ok(format!("{}/design/", html_url.trim_end_matches('/')))
}

fn failed(what: &str, repo: &Repo, output: &Output) -> Error {
    Error::internal(
        format!(
            "could not {what} the GitHub Pages site of {repo}: {}",
            output.stderr.trim()
        ),
        format!(
            "Pages needs a public repository and an account that administers it (`gh auth status`); set Pages by hand to the `{BRANCH}` branch, folder `{FOLDER}`"
        ),
    )
}

fn run_gh(root: &Path, args: &[&str]) -> Result<Output> {
    run(root, "gh", args).ok_or_else(|| {
        Error::internal(
            "could not run `gh`, the GitHub CLI",
            format!(
                "install gh and run `gh auth login`, or set GitHub Pages by hand to the `{BRANCH}` branch, folder `{FOLDER}`"
            ),
        )
    })
}

fn git(root: &Path, args: &[&str]) -> Option<Output> {
    run(root, "git", args)
}

/// Runs `program` in `root`; `None` when it cannot start.
fn run(root: &Path, program: &str, args: &[&str]) -> Option<Output> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    Some(Output {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn repo() -> Repo {
        Repo::from_remote("https://github.com/payangar-studio/swift-blades.git").unwrap()
    }

    fn ok(stdout: &str) -> Output {
        Output {
            success: true,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    fn err(stderr: &str) -> Output {
        Output {
            success: false,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }

    /// Runs `configure` against answers given in order; returns its result
    /// and the calls, one string each.
    fn drive(answers: Vec<Output>) -> (Result<String>, Vec<String>) {
        let answers = RefCell::new(answers.into_iter());
        let calls = RefCell::new(Vec::new());
        let result = configure(&repo(), |args| {
            calls.borrow_mut().push(args.join(" "));
            Ok(answers.borrow_mut().next().expect("an unexpected gh call"))
        });
        (result, calls.into_inner())
    }

    const SITE: &str = "https://payangar-studio.github.io/swift-blades/";

    #[test]
    fn remote_urls_name_the_repository() {
        for url in [
            "https://github.com/payangar-studio/swift-blades",
            "https://github.com/payangar-studio/swift-blades.git\n",
            "git@github.com:payangar-studio/swift-blades.git",
            "ssh://git@github.com/payangar-studio/swift-blades.git",
        ] {
            assert_eq!(Repo::from_remote(url), Some(repo()), "{url}");
        }
        for url in [
            "https://gitlab.com/payangar-studio/swift-blades.git",
            "/tmp/bare.git",
            "https://github.com/payangar-studio",
            "https://github.com/a/b/c",
        ] {
            assert_eq!(Repo::from_remote(url), None, "{url}");
        }
    }

    #[test]
    fn a_repository_without_pages_gets_a_site_from_the_design_branch() {
        let (url, calls) = drive(vec![
            err("gh: Not Found (HTTP 404)"),
            ok(&format!("legacy design /docs {SITE}")),
        ]);
        assert_eq!(url.unwrap(), format!("{SITE}design/"));
        assert_eq!(calls.len(), 2);
        assert!(
            calls[1].starts_with("api --method POST repos/payangar-studio/swift-blades/pages ")
        );
        for field in [
            "source[branch]=design",
            "source[path]=/docs",
            "build_type=legacy",
        ] {
            assert!(
                calls[1].contains(field),
                "{field} missing from {}",
                calls[1]
            );
        }
    }

    #[test]
    fn a_site_already_on_the_design_branch_is_left_alone() {
        let (url, calls) = drive(vec![ok(&format!("legacy design /docs {SITE}"))]);
        assert_eq!(url.unwrap(), format!("{SITE}design/"));
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn a_site_from_another_source_is_moved_to_the_design_branch() {
        let (url, calls) = drive(vec![ok(&format!("legacy 26.3 / {SITE}")), ok("")]);
        assert_eq!(url.unwrap(), format!("{SITE}design/"));
        assert!(calls[1].starts_with("api --method PUT repos/payangar-studio/swift-blades/pages "));
        assert!(calls[1].contains("source[branch]=design"));

        let (_, calls) = drive(vec![ok(&format!("workflow design /docs {SITE}")), ok("")]);
        assert!(calls[1].contains("build_type=legacy"), "{calls:?}");
    }

    #[test]
    fn a_refusal_of_github_is_reported() {
        let (result, _) = drive(vec![
            err("gh: Not Found (HTTP 404)"),
            err(
                "gh: Your current plan does not support GitHub Pages for this repository. (HTTP 422)",
            ),
        ]);
        let error = result.unwrap_err();
        assert!(error.to_string().contains("HTTP 422"), "{error}");
        assert!(error.hint().contains("public repository"));

        let (result, calls) = drive(vec![err("gh: Bad credentials (HTTP 401)")]);
        assert!(result.unwrap_err().to_string().contains("HTTP 401"));
        assert_eq!(calls.len(), 1);
    }
}
