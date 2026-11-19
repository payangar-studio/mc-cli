//! The GitHub repository of a mod, configured through the REST API by
//! `mod setup`: the ruleset that reserves the release tags, the repository
//! variables the publish workflow reads to reach the vault, and the
//! `CODEOWNERS` file that covers the workflows.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use ureq::Agent;

use crate::error::{Context as _, Error, Result};

/// The public API.
pub const API: &str = "https://api.github.com";

/// Name of the ruleset that reserves the release tags.
pub const RULESET: &str = "Release tags";

/// The variables of the vault: name, then what it holds, as a question.
pub const VARIABLES: [(&str, &str); 2] = [
    ("INFISICAL_URL", "URL of the vault"),
    (
        "INFISICAL_IDENTITY_ID",
        "Id of the vault identity of the CI (`ci-minecraft`)",
    ),
];

/// Where the owners of the workflows are declared.
pub const CODEOWNERS: &str = ".github/CODEOWNERS";

const USER_AGENT: &str = concat!("payangar-studio/mc-cli/", env!("CARGO_PKG_VERSION"));
const AUTH_HINT: &str = "run `gh auth login` with an account that administers the repository, or set GH_TOKEN; pass --no-github to skip GitHub";

/// A repository, as `owner/name`.
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
        let url = url.trim();
        let rest = [
            "https://github.com/",
            "git@github.com:",
            "ssh://git@github.com/",
        ]
        .iter()
        .find_map(|prefix| url.strip_prefix(prefix))?;
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

/// What GitHub says of a repository, as far as setup needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepoInfo {
    pub private: bool,
}

/// A client of the GitHub REST API, authenticated by a token.
pub struct GitHub {
    agent: Agent,
    api: String,
    token: String,
}

impl GitHub {
    pub fn new(api: &str, token: String) -> Self {
        let agent = Agent::config_builder()
            .user_agent(USER_AGENT)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(60)))
            .build()
            .into();
        Self {
            agent,
            api: api.trim_end_matches('/').to_owned(),
            token,
        }
    }

    /// Sends `method path` with an optional JSON body; returns the status and
    /// the JSON answer (`null` when empty).
    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<(u16, Value)> {
        let url = format!("{}{path}", self.api);
        let auth = format!("Bearer {}", self.token);
        let what = format!("calling GitHub ({method} {path})");
        macro_rules! with_headers {
            ($req:expr) => {
                $req.header("Authorization", &auth)
                    .header("Accept", "application/vnd.github+json")
                    .header("X-GitHub-Api-Version", "2022-11-28")
            };
        }
        let response = match (method, body) {
            ("GET", _) => with_headers!(self.agent.get(&url)).call(),
            ("POST", Some(b)) => with_headers!(self.agent.post(&url)).send_json(b),
            ("PATCH", Some(b)) => with_headers!(self.agent.patch(&url)).send_json(b),
            ("PUT", Some(b)) => with_headers!(self.agent.put(&url)).send_json(b),
            _ => unreachable!("{method} without a body is not used"),
        };
        let mut response = response.context(what.clone())?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .read_to_string()
            .context(format!("reading the answer of GitHub to {method} {path}"))?;
        let value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        match status {
            401 => Err(Error::usage(
                "GitHub refused the token (401 Unauthorized)",
                AUTH_HINT,
            )),
            403 => Err(Error::usage(
                format!(
                    "GitHub refused {method} {path} (403 Forbidden): {}",
                    message(&value)
                ),
                AUTH_HINT,
            )),
            _ => Ok((status, value)),
        }
    }

    /// The repository; `None` when it does not exist, or the token cannot see it.
    pub fn repository(&self, repo: &Repo) -> Result<Option<RepoInfo>> {
        let path = format!("/repos/{repo}");
        let (status, value) = self.call("GET", &path, None)?;
        match status {
            404 => Ok(None),
            200 => Ok(Some(RepoInfo {
                private: value["private"].as_bool().unwrap_or(true),
            })),
            _ => Err(unexpected("GET", &path, status, &value)),
        }
    }

    /// The login of the token's account.
    pub fn login(&self) -> Result<String> {
        let (status, value) = self.call("GET", "/user", None)?;
        match (status, value["login"].as_str()) {
            (200, Some(login)) => Ok(login.to_owned()),
            _ => Err(unexpected("GET", "/user", status, &value)),
        }
    }

    /// The value of a repository variable; `None` when it is not set.
    pub fn variable(&self, repo: &Repo, name: &str) -> Result<Option<String>> {
        let path = format!("/repos/{repo}/actions/variables/{name}");
        let (status, value) = self.call("GET", &path, None)?;
        match status {
            404 => Ok(None),
            200 => Ok(value["value"].as_str().map(str::to_owned)),
            _ => Err(unexpected("GET", &path, status, &value)),
        }
    }

    /// Sets a repository variable: updates it, or creates it when missing.
    pub fn set_variable(&self, repo: &Repo, name: &str, value: &str) -> Result<()> {
        let path = format!("/repos/{repo}/actions/variables/{name}");
        let body = json!({ "name": name, "value": value });
        let (status, answer) = self.call("PATCH", &path, Some(&body))?;
        match status {
            204 | 200 => return Ok(()),
            404 => {}
            _ => return Err(unexpected("PATCH", &path, status, &answer)),
        }
        let path = format!("/repos/{repo}/actions/variables");
        let (status, answer) = self.call("POST", &path, Some(&body))?;
        match status {
            201 | 200 | 204 => Ok(()),
            _ => Err(unexpected("POST", &path, status, &answer)),
        }
    }

    /// Creates or updates the ruleset [`RULESET`], found by its name among
    /// the repository's own rulesets. Returns whether it was created.
    pub fn protect_release_tags(&self, repo: &Repo) -> Result<bool> {
        let path = format!("/repos/{repo}/rulesets");
        let list = format!("{path}?includes_parents=false&per_page=100");
        let (status, value) = self.call("GET", &list, None)?;
        if status != 200 {
            return Err(unexpected("GET", &list, status, &value));
        }
        let existing = value.as_array().and_then(|rulesets| {
            rulesets
                .iter()
                .find(|r| r["name"].as_str() == Some(RULESET))
                .and_then(|r| r["id"].as_u64())
        });
        let body = release_tags_ruleset();
        let (method, path, created) = match existing {
            Some(id) => ("PUT", format!("{path}/{id}"), false),
            None => ("POST", path, true),
        };
        let (status, answer) = self.call(method, &path, Some(&body))?;
        match status {
            200 | 201 => Ok(created),
            _ => Err(unexpected(method, &path, status, &answer)),
        }
    }
}

/// Configures `repo` for the mod at `root` and returns what it did, one line
/// each. Missing on GitHub, the repository is left alone.
///
/// - Each variable of [`VARIABLES`] takes the value given for it; without
///   one, a variable already set is kept, and a missing one gets the answer
///   of `ask(name, question)`.
/// - The ruleset of the release tags is created or updated. A private
///   repository is skipped: rulesets need a paid plan there.
/// - `CODEOWNERS` names the token's account, unless the file exists.
pub fn configure(
    gh: &GitHub,
    repo: &Repo,
    root: &Path,
    given: [Option<String>; 2],
    mut ask: impl FnMut(&str, &str) -> Result<String>,
) -> Result<Vec<String>> {
    let mut done = Vec::new();
    let Some(info) = gh.repository(repo)? else {
        done.push(format!(
            "{repo} does not exist on GitHub, or this account cannot see it: nothing configured. Create it, push, then run `mod setup` again."
        ));
        return Ok(done);
    };
    for ((name, question), value) in VARIABLES.into_iter().zip(given) {
        let value = match value {
            Some(value) => value,
            None => match gh.variable(repo, name)? {
                Some(_) => {
                    done.push(format!("Kept the variable {name}."));
                    continue;
                }
                None => ask(name, question)?,
            },
        };
        if value.trim().is_empty() {
            return Err(Error::usage(
                format!("the value of {name} is empty"),
                format!("pass the value of {name}"),
            ));
        }
        gh.set_variable(repo, name, value.trim())?;
        done.push(format!("Set the variable {name}."));
    }
    if info.private {
        tracing::warn!(
            "{repo} is private: rulesets need a paid plan there, the release tags are not protected"
        );
        done.push("Skipped the ruleset of the release tags: the repository is private.".into());
    } else if gh.protect_release_tags(repo)? {
        done.push(format!("Created the ruleset `{RULESET}`."));
    } else {
        done.push(format!("Updated the ruleset `{RULESET}`."));
    }
    if root.join(CODEOWNERS).exists() {
        done.push(format!("Kept {CODEOWNERS}."));
    } else {
        let login = gh.login()?;
        write_codeowners(root, &login)?;
        done.push(format!(
            "Wrote {CODEOWNERS} with @{login} as the owner of the workflows: commit it."
        ));
    }
    Ok(done)
}

/// The ruleset that reserves the creation, update and deletion of `v*` tags
/// to the administrators of the repository and of the organization: a tag
/// publishes the mod, and the vault trusts any tag of the organization.
pub fn release_tags_ruleset() -> Value {
    json!({
        "name": RULESET,
        "target": "tag",
        "enforcement": "active",
        "conditions": {
            "ref_name": { "include": ["refs/tags/v*"], "exclude": [] }
        },
        "rules": [
            { "type": "creation" },
            { "type": "update", "parameters": { "update_allows_fetch_and_merge": false } },
            { "type": "deletion" }
        ],
        "bypass_actors": [
            // Repository role 5 is the admin role.
            { "actor_type": "RepositoryRole", "actor_id": 5, "bypass_mode": "always" },
            { "actor_type": "OrganizationAdmin", "actor_id": null, "bypass_mode": "always" }
        ]
    })
}

/// The `CODEOWNERS` file for `login`: the workflows and the file itself.
pub fn codeowners(login: &str) -> String {
    format!(
        "# The workflows run with the repository's permissions and reach the vault:\n\
         # a change to them needs the review of their owner.\n\
         /.github/workflows/ @{login}\n\
         /.github/CODEOWNERS @{login}\n"
    )
}

/// Writes [`codeowners`] at the root of the repository.
fn write_codeowners(root: &Path, login: &str) -> Result<()> {
    let path = root.join(".github").join("CODEOWNERS");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).context(format!("creating {}", dir.display()))?;
    }
    std::fs::write(&path, codeowners(login)).context(format!("writing {}", path.display()))
}

fn message(value: &Value) -> String {
    value["message"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn unexpected(method: &str, path: &str, status: u16, value: &Value) -> Error {
    Error::internal(
        format!(
            "GitHub answered {status} to {method} {path}: {}",
            message(value)
        ),
        "check https://www.githubstatus.com, then run `mod setup` again",
    )
}

#[cfg(test)]
mod tests;
