//! Mod resolution: from a file to its entry, by asking every platform that
//! knows its hashes; from a slug, an id or a URL to the platform that knows
//! the project, and its versions compatible with a [`Pack`].

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::{Context, Error, Result};
use crate::hash::{JarHashes, hash_file};
use crate::model::entry::{Entry, Hashes, Kind, Pack, Source};
use crate::platform::{Candidate, Jar, Platform, Registry};

/// An entry being built, before it gets its key in the project's list.
#[derive(Debug, Clone)]
pub struct Draft {
    pub entry: Entry,
    /// Key to keep: the entry replaces the one with this key.
    pub key: Option<String>,
    /// Project slug given by each platform that knows the jar.
    pub slugs: BTreeMap<String, Option<String>>,
}

impl Draft {
    /// A draft for the file at `path` (`<kind dir>/<name>`, or
    /// `<kind local dir>/<name>` for a local entry), from its hashes.
    pub fn from_hashes(path: &str, hashes: &JarHashes, key: Option<String>) -> Self {
        let (kind, local) =
            Kind::of_path(path).expect("drafts are built for files of a kind's directory");
        Self {
            entry: Entry {
                key: key.clone().unwrap_or_else(|| file_name(path).to_owned()),
                kind,
                path: path.to_owned(),
                file_size: hashes.size,
                downloads: Vec::new(),
                url: None,
                source: local.then_some(Source::Local),
                approved: None,
                hashes: Hashes {
                    sha1: hashes.sha1.clone(),
                    sha512: hashes.sha512.clone(),
                },
                env: kind.default_env(),
                platforms: BTreeMap::new(),
            },
            key,
            slugs: BTreeMap::new(),
        }
    }

    /// A draft for the file at `path` under `root`, hashing it.
    pub fn from_jar(root: &Path, path: &str, key: Option<String>) -> Result<Self> {
        let file = root.join(path);
        let hashes = hash_file(&file).context(format!("reading {}", file.display()))?;
        Ok(Self::from_hashes(path, &hashes, key))
    }

    /// Whether some platform knows the entry.
    pub fn has_source(&self) -> bool {
        !self.entry.platforms.is_empty()
    }
}

/// Asks every platform without a table in a draft to identify its file, then
/// lets each platform that recognized one complete the entry. Local entries
/// are left out. Returns the warnings to show.
pub fn complete<P: Platform + ?Sized>(
    root: &Path,
    drafts: &mut [Draft],
    registry: &Registry<P>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for platform in registry.all() {
        let name = platform.name();
        let pending: Vec<usize> = (0..drafts.len())
            .filter(|&i| !drafts[i].entry.is_local() && drafts[i].entry.platform(name).is_none())
            .collect();
        if pending.is_empty() {
            continue;
        }
        let jars: Vec<Jar> = pending
            .iter()
            .map(|&i| {
                let e = &drafts[i].entry;
                Jar {
                    path: root.join(&e.path),
                    sha1: e.hashes.sha1.clone(),
                    sha512: e.hashes.sha512.clone(),
                    size: e.file_size,
                }
            })
            .collect();
        let found = match platform.identify(&jars) {
            Ok(found) => found,
            Err(err) => {
                warnings.push(format!("{name} skipped: {}", first_line(&err)));
                continue;
            }
        };
        for (&i, found) in pending.iter().zip(found) {
            let Some(found) = found else { continue };
            let draft = &mut drafts[i];
            if !found.exact {
                warnings.push(format!(
                    "{} matches a {name} file only partly; not linked to {name}",
                    draft.entry.path
                ));
                continue;
            }
            draft.entry.platforms.insert(name.to_owned(), found.section);
            draft.slugs.insert(name.to_owned(), found.slug);
            if let Err(err) = platform.fill_entry(&mut draft.entry) {
                warnings.push(format!(
                    "{name} could not complete {}: {}",
                    draft.entry.path,
                    first_line(&err)
                ));
            }
        }
    }
    warnings
}

/// The platform that knows `query` and its candidates: `on` alone when given,
/// else the first platform of the registry with a candidate.
pub fn resolve<'a, P: Platform + ?Sized>(
    registry: &'a Registry<P>,
    on: Option<&'a P>,
    query: &str,
    pack: &Pack,
) -> Result<(&'a P, Vec<Candidate>)> {
    let platforms: Vec<&P> = match on {
        Some(p) => vec![p],
        None => registry.all().collect(),
    };
    let mut failure = None;
    for platform in platforms {
        match platform.resolve(query, pack) {
            Ok(candidates) if !candidates.is_empty() => return Ok((platform, candidates)),
            Ok(_) => {}
            Err(err) => {
                tracing::debug!(platform = platform.name(), "resolve failed");
                failure.get_or_insert(err);
            }
        }
    }
    Err(failure.unwrap_or_else(|| {
        Error::usage(
            format!(
                "no version of `{query}` for Minecraft {} with {}",
                pack.minecraft, pack.loader
            ),
            "check the slug or URL, or add a local file",
        )
    }))
}

/// The first line of an error, without its `error: ` prefix.
pub fn first_line(err: &Error) -> String {
    let text = err.to_string();
    let line = text.lines().next().unwrap_or_default();
    line.strip_prefix("error: ").unwrap_or(line).to_owned()
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
