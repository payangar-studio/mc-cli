//! GitHub releases as a platform: a mod or a resource pack published as an
//! asset of a repository's release, asked for as
//! `github:<owner>/<repo>[@<tag>[/<file>]]`. Reading needs no token; GitHub
//! allows 60 anonymous API requests an hour.
//!
//! Its manifest table holds `repo` (`<owner>/<repo>`), `tag` and `file`, the
//! asset's name. GitHub gives no sha512, so the file is downloaded once into
//! the machine cache to compute its hashes, and served from there.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use ureq::Agent;
use ureq::http::StatusCode;

use super::modrinth::encode_segment;
use super::{Candidate, Capability, Download, Jar, Match, Operation, Platform};
use crate::error::{Context, Error, Result};
use crate::hash::{copy_hashing, hash_file};
use crate::model::entry::{Entry, Hashes, Kind, Pack};

pub const NAME: &str = "github";
pub const API_URL: &str = "https://api.github.com";
/// What starts a query for this platform.
pub const PREFIX: &str = "github:";
const USER_AGENT: &str = concat!("payangar-studio/mc-cli/", env!("CARGO_PKG_VERSION"));

pub struct GitHub {
    agent: Agent,
    /// Base URL of the API, without trailing slash.
    api: String,
    /// Where downloaded assets are kept, `<repo>/<tag>/<file>` under it.
    cache: PathBuf,
    /// The releases already read, by repository and tag: resolving then
    /// filling an entry asks the API once.
    seen: Mutex<BTreeMap<(String, String), Release>>,
}

impl Default for GitHub {
    /// The GitHub API, assets cached in `<user cache dir>/mc-cli/github`,
    /// shared by the binaries.
    fn default() -> Self {
        let cache = directories::BaseDirs::new()
            .map_or_else(std::env::temp_dir, |d| d.cache_dir().to_path_buf())
            .join("mc-cli")
            .join(NAME);
        Self::new(API_URL, cache)
    }
}

impl GitHub {
    /// A client of the API at `api` (tests point it to a mock server).
    pub fn new(api: &str, cache: PathBuf) -> Self {
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
            cache,
            seen: Mutex::new(BTreeMap::new()),
        }
    }

    /// `GET <api><path>`; `None` on 404.
    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        let url = format!("{}{path}", self.api);
        let mut response = self
            .agent
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .call()
            .context(format!("calling GitHub (GET {path})"))?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        if (status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS)
            && header("x-ratelimit-remaining").as_deref() == Some("0")
        {
            return Err(rate_limited(header("x-ratelimit-reset")));
        }
        if !status.is_success() {
            return Err(Error::internal(
                format!("GitHub answered {status} to GET {path}"),
                "retry later; if it persists, check https://www.githubstatus.com",
            ));
        }
        let value = response
            .body_mut()
            .read_json()
            .context(format!("reading the answer of GitHub to GET {path}"))?;
        Ok(Some(value))
    }

    fn release(&self, repo: &str, tag: &str) -> Result<Release> {
        let key = (repo.to_owned(), tag.to_owned());
        if let Some(release) = self.seen().get(&key) {
            return Ok(release.clone());
        }
        let release: Release = self
            .get(&format!(
                "/repos/{repo}/releases/tags/{}",
                encode_segment(tag)
            ))?
            .ok_or_else(|| {
                Error::usage(
                    format!("GitHub has no release `{tag}` of `{repo}`"),
                    format!("check the tag on https://github.com/{repo}/releases"),
                )
            })?;
        self.seen().insert(key, release.clone());
        Ok(release)
    }

    fn releases(&self, repo: &str) -> Result<Vec<Release>> {
        let releases: Vec<Release> = self
            .get(&format!("/repos/{repo}/releases?per_page=100"))?
            .ok_or_else(|| {
                Error::usage(
                    format!("GitHub has no public repository `{repo}`"),
                    "write it as `github:<owner>/<repo>`, as in its URL on github.com",
                )
            })?;
        let mut seen = self.seen();
        for release in &releases {
            seen.insert((repo.to_owned(), release.tag_name.clone()), release.clone());
        }
        Ok(releases)
    }

    fn seen(&self) -> std::sync::MutexGuard<'_, BTreeMap<(String, String), Release>> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The cached copy of the asset `file` of release `tag` of `repo`,
    /// downloaded from `url` when missing or of another size than `size`.
    fn cached(&self, repo: &str, tag: &str, file: &str, url: &str, size: u64) -> Result<PathBuf> {
        let dir = self.cache.join(repo).join(encode_segment(tag));
        let path = dir.join(file);
        if fs::metadata(&path).is_ok_and(|m| m.len() == size) {
            return Ok(path);
        }
        fs::create_dir_all(&dir).context(format!("creating {}", dir.display()))?;
        let response = self
            .agent
            .get(url)
            .call()
            .context(format!("downloading {url}"))?;
        if !response.status().is_success() {
            return Err(Error::internal(
                format!(
                    "GitHub answered {} to the download of {url}",
                    response.status()
                ),
                "retry later; if it persists, check https://www.githubstatus.com",
            ));
        }
        let part = dir.join(format!("{file}.part"));
        let mut out = fs::File::create(&part).context(format!("writing {}", part.display()))?;
        let copied = copy_hashing(&mut response.into_body().into_reader(), &mut out);
        drop(out);
        match copied {
            Ok(hashes) if hashes.size == size => {}
            Ok(hashes) => {
                let _ = fs::remove_file(&part);
                return Err(Error::internal(
                    format!(
                        "the download of {url} holds {} bytes, GitHub announced {size}",
                        hashes.size
                    ),
                    "run the command again",
                ));
            }
            Err(e) => {
                let _ = fs::remove_file(&part);
                return Err(Error::internal(
                    format!("cannot download {url}: {e}"),
                    "check the network connection, then run the command again",
                ));
            }
        }
        fs::rename(&part, &path).context(format!("writing {}", path.display()))?;
        Ok(path)
    }
}

impl Platform for GitHub {
    fn name(&self) -> &str {
        NAME
    }

    fn capability(&self, op: Operation) -> Capability {
        match op {
            Operation::Identify
            | Operation::Resolve
            | Operation::FillEntry
            | Operation::Download => Capability::Api,
            Operation::PackEligibility | Operation::BuildPack | Operation::PushPage => {
                Capability::Unsupported
            }
        }
    }

    /// GitHub cannot find a file by its hashes: it recognizes none.
    fn identify(&self, jars: &[Jar]) -> Result<Vec<Option<Match>>> {
        Ok(vec![None; jars.len()])
    }

    fn resolve(&self, query: &str, pack: &Pack) -> Result<Vec<Candidate>> {
        let Some(query) = Query::parse(query)? else {
            return Ok(Vec::new());
        };
        let releases = match &query.tag {
            Some(tag) => vec![self.release(&query.repo, tag)?],
            None => {
                let mut releases = self.releases(&query.repo)?;
                releases.retain(|r| !r.draft);
                // GitHub lists the newest first; releases go before prereleases.
                releases.sort_by_key(|r| r.prerelease);
                // A repository that names Minecraft versions in its releases
                // offers those of the pack's version only.
                if releases.iter().any(|r| r.mentions(&pack.minecraft)) {
                    releases.retain(|r| r.mentions(&pack.minecraft));
                }
                releases
            }
        };
        let mut candidates = Vec::new();
        for release in &releases {
            let files = release.pick(query.file.as_deref(), &pack.loader);
            match files.as_slice() {
                [] => {}
                [asset] => candidates.push(query.candidate(release, asset)?),
                several if candidates.is_empty() => {
                    let names: Vec<&str> = several.iter().map(|a| a.name.as_str()).collect();
                    return Err(Error::usage(
                        format!(
                            "release `{}` of `{}` has several files that could be meant: {}",
                            release.tag_name,
                            query.repo,
                            names.join(", ")
                        ),
                        format!(
                            "name the file: `github:{}@{}/<file>`",
                            query.repo, release.tag_name
                        ),
                    ));
                }
                // An older release that cannot be told apart is left out.
                _ => {}
            }
        }
        if candidates.is_empty() {
            let which = match &query.tag {
                Some(tag) => format!("release `{tag}` of `{}`", query.repo),
                None => format!("no release of `{}`", query.repo),
            };
            let what = match (&query.tag, &query.file) {
                (_, Some(file)) => format!("{which} has no file `{file}`"),
                (Some(_), None) => format!("{which} has no .jar nor .zip file"),
                (None, None) => format!(
                    "{which} has a .jar or a .zip file for Minecraft {}",
                    pack.minecraft
                ),
            };
            return Err(Error::usage(
                what,
                format!(
                    "check the files of its releases on https://github.com/{}/releases",
                    query.repo
                ),
            ));
        }
        Ok(candidates)
    }

    /// Takes the download URL from the release, and the hashes and size from
    /// the cached copy when the entry has none yet. The sides stay those of a
    /// file no platform describes.
    fn fill_entry(&self, entry: &mut Entry) -> Result<()> {
        let at = At::of(entry)?;
        let release = self.release(&at.repo, &at.tag)?;
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == at.file)
            .ok_or_else(|| at.gone(&entry.key))?;
        entry.downloads = vec![asset.browser_download_url.clone()];
        if entry.hashes.sha512.is_empty() {
            let path = self.cached(
                &at.repo,
                &at.tag,
                &asset.name,
                &asset.browser_download_url,
                asset.size,
            )?;
            let hashes = hash_file(&path).context(format!("reading {}", path.display()))?;
            if let Some(digest) = asset
                .digest
                .as_deref()
                .and_then(|d| d.strip_prefix("sha256:"))
                && !digest.eq_ignore_ascii_case(&hashes.sha256)
            {
                let _ = fs::remove_file(&path);
                return Err(Error::internal(
                    format!(
                        "the download of `{}` does not match the sha256 GitHub gives",
                        entry.key
                    ),
                    "run the command again",
                ));
            }
            entry.hashes = Hashes {
                sha1: hashes.sha1,
                sha512: hashes.sha512,
            };
            entry.file_size = hashes.size;
        }
        Ok(())
    }

    /// The cached copy, downloaded first when missing. The caller checks it
    /// against the entry's sha512.
    fn download(&self, entry: &Entry) -> Result<Download> {
        let at = At::of(entry)?;
        let url = entry.downloads.first().ok_or_else(|| at.gone(&entry.key))?;
        let path = self.cached(&at.repo, &at.tag, &at.file, url, entry.file_size)?;
        let file = fs::File::open(&path).context(format!("reading {}", path.display()))?;
        Ok(Download::Stream(Box::new(file)))
    }
}

/// The rate limit error: when it resets, from `X-RateLimit-Reset` (seconds
/// since the epoch).
fn rate_limited(reset: Option<String>) -> Error {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hint = match reset.and_then(|r| r.trim().parse::<u64>().ok()) {
        Some(at) => format!(
            "it resets in {} minutes: run the command again then",
            at.saturating_sub(now).div_ceil(60).max(1)
        ),
        None => "it resets within the hour: run the command again then".to_owned(),
    };
    Error::internal(
        "GitHub's API rate limit for this machine is reached (60 requests an hour)",
        hint,
    )
}

/// What a query asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Query {
    /// `<owner>/<repo>`.
    repo: String,
    tag: Option<String>,
    /// The asset's name, when the release holds several candidates.
    file: Option<String>,
}

impl Query {
    /// `None` when the query is not for GitHub. After the tag, a last
    /// segment with the extension of a mod or a resource pack names the file;
    /// otherwise the whole is the tag, which may hold a `/`.
    fn parse(query: &str) -> Result<Option<Self>> {
        let Some(rest) = query.trim().strip_prefix(PREFIX) else {
            return Ok(None);
        };
        let (repo, at) = match rest.split_once('@') {
            Some((repo, at)) => (repo, Some(at)),
            None => (rest, None),
        };
        if !valid_repo(repo) || at.is_some_and(str::is_empty) {
            return Err(Error::usage(
                format!("`{query}` is not a GitHub repository"),
                "write `github:<owner>/<repo>`, optionally followed by `@<tag>` and `/<file>`, such as `github:CaffeineMC/lithium@mc26.3-0.26.4`",
            ));
        }
        let (tag, file) = match at {
            None => (None, None),
            Some(at) => match at.rsplit_once('/') {
                Some((tag, file)) if !tag.is_empty() && Kind::of_file_name(file).is_some() => {
                    (Some(tag.to_owned()), Some(file.to_owned()))
                }
                _ => (Some(at.to_owned()), None),
            },
        };
        Ok(Some(Self {
            repo: repo.to_owned(),
            tag,
            file,
        }))
    }

    fn candidate(&self, release: &Release, asset: &Asset) -> Result<Candidate> {
        let kind = Kind::of_file_name(&asset.name).ok_or_else(|| {
            Error::usage(
                format!(
                    "`{}` of `{}` is neither a mod (.jar) nor a resource pack (.zip)",
                    asset.name, self.repo
                ),
                "name a .jar or a .zip file of the release",
            )
        })?;
        let mut section = toml::Table::new();
        section.insert("repo".into(), self.repo.clone().into());
        section.insert("tag".into(), release.tag_name.clone().into());
        section.insert("file".into(), asset.name.clone().into());
        Ok(Candidate {
            label: release.tag_name.clone(),
            slug: self.repo.split('/').nth(1).map(str::to_owned),
            kind,
            file_name: asset.name.clone(),
            section,
        })
    }
}

/// Whether `repo` is `<owner>/<repo>` as GitHub writes them.
fn valid_repo(repo: &str) -> bool {
    repo.split('/').count() == 2
        && repo.split('/').all(|s| {
            !s.is_empty()
                && s != "."
                && s != ".."
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

/// Checks that `query`, which starts with [`PREFIX`], is a GitHub query.
pub fn check_query(query: &str) -> Result<()> {
    Query::parse(query).map(|_| ())
}

/// Where an entry's table points.
struct At {
    repo: String,
    tag: String,
    file: String,
}

impl At {
    fn of(entry: &Entry) -> Result<Self> {
        let table = entry.platform(NAME);
        let field = |name: &str| {
            table
                .and_then(|t| t.get(name))
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| {
                    Error::usage(
                        format!("`{}` has no GitHub release", entry.key),
                        "get it from another platform, or give its direct URL",
                    )
                })
        };
        let at = Self {
            repo: field("repo")?,
            tag: field("tag")?,
            file: field("file")?,
        };
        // The repository and the file name a path under the cache.
        if !valid_repo(&at.repo)
            || at.file.is_empty()
            || at.file.contains(['/', '\\'])
            || at.file == "."
            || at.file == ".."
        {
            return Err(Error::usage(
                format!(
                    "the GitHub table of `{}` names `{}` of `{}`, not a release file",
                    entry.key, at.file, at.repo
                ),
                format!("add `{}` again from its GitHub release", entry.key),
            ));
        }
        Ok(at)
    }

    fn gone(&self, key: &str) -> Error {
        Error::usage(
            format!(
                "release `{}` of `{}` no longer has the file `{}` of `{key}`",
                self.tag, self.repo, self.file
            ),
            format!("add `{key}` again to pick an available release"),
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

impl Release {
    /// Whether its tag, title or a file name names the version `minecraft`.
    fn mentions(&self, minecraft: &str) -> bool {
        names_version(&self.tag_name, minecraft)
            || self
                .name
                .as_deref()
                .is_some_and(|n| names_version(n, minecraft))
            || self
                .assets
                .iter()
                .any(|a| names_version(&a.name, minecraft))
    }

    /// The files `file` can mean: that file when named; else the jars, or the
    /// zips when there is no jar, without sources and javadoc, narrowed to
    /// those whose name holds `loader` when some do.
    fn pick(&self, file: Option<&str>, loader: &str) -> Vec<&Asset> {
        if let Some(file) = file {
            return self.assets.iter().filter(|a| a.name == file).collect();
        }
        let of = |kind: Kind| -> Vec<&Asset> {
            self.assets
                .iter()
                .filter(|a| Kind::of_file_name(&a.name) == Some(kind))
                .filter(|a| {
                    let name = a.name.to_ascii_lowercase();
                    !name.ends_with("-sources.jar") && !name.ends_with("-javadoc.jar")
                })
                .collect()
        };
        let mut files = of(Kind::Mod);
        if files.is_empty() {
            files = of(Kind::Resourcepack);
        }
        let loader = loader.to_ascii_lowercase();
        let with_loader: Vec<&Asset> = files
            .iter()
            .copied()
            .filter(|a| a.name.to_ascii_lowercase().contains(&loader))
            .collect();
        if files.len() > 1 && !with_loader.is_empty() {
            files = with_loader;
        }
        files
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
    /// `sha256:<hex>`; absent on assets uploaded before GitHub computed it.
    #[serde(default)]
    digest: Option<String>,
}

/// Whether `text` holds `version` as a whole version: `1.21` is in
/// `mod-1.21-2.0.jar` and `mc1.21`, not in `mod-1.21.1.jar` nor `11.21`.
fn names_version(text: &str, version: &str) -> bool {
    if version.is_empty() {
        return false;
    }
    let numeric = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit());
    text.match_indices(version).any(|(at, _)| {
        let mut before = text[..at].chars().rev();
        let mut after = text[at + version.len()..].chars();
        let (b, a) = (before.next(), after.next());
        let continues_before = numeric(b) || (b == Some('.') && numeric(before.next()));
        let continues_after = numeric(a) || (a == Some('.') && numeric(after.next()));
        !continues_before && !continues_after
    })
}
