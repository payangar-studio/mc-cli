//! The socle mechanics, after the Spec Kit model: what `init` lays down and
//! `update` rewrites in a repository, recorded in a registry with hashes so
//! that a hand edit is never overwritten silently.
//!
//! A project type describes its socle with a [`Socle`]: its directory, its
//! templates (embedded in its binary), and how messages name its CLI. The
//! templates come in three families:
//! - `files`: files the socle owns, written at the same relative path,
//!   recorded in the registry and rewritten by every update;
//! - `blocks`: blocks between markers in a file the project owns otherwise,
//!   recorded in the registry as blocks;
//! - `project`: the skeleton of the project's content, written once by
//!   `init` when missing, never recorded, never touched again.
//!
//! A socle may also lay out files by code (`laid_out`), when their path or
//! content depend on the repository: they are written, recorded and rewritten
//! like `files`.
//!
//! Every socle also owns the files of [`SHARED`] (`templates/socle/` of this
//! crate): the skills that serve every project type, such as the CurseForge
//! page. They are written, recorded and rewritten with the socle's own files.

pub mod blocks;

use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Once;

use include_dir::{Dir, include_dir};
use semver::Version;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{Context, Error, Result};
use crate::model::socle::{
    LockedFile, LockedKind, SocleConfig, SocleLock, VersionCheck, compare_versions,
};
use crate::model::{sha256_bytes, sha256_file};
use blocks::Markers;

/// Files every socle owns, whatever the project type, at their path in the
/// repository.
pub static SHARED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/socle");

/// The instructions file, and the link that points Claude at it.
const AGENTS_MD: &str = "AGENTS.md";
const CLAUDE_MD: &str = "CLAUDE.md";

/// A project type's socle: where it sits in a repository, what it lays down,
/// and how messages name its CLI.
pub struct Socle {
    /// Directory at the root of a repository of this type, which marks it
    /// (`.modpack`). It holds the socle settings and registry.
    pub dir: &'static str,
    /// The binary, as messages name its commands (`modpack`).
    pub bin: &'static str,
    /// The CLI, as file headers and upgrade hints name it (`mc-cli`).
    pub tool: &'static str,
    /// Version of the CLI, which is also the version of the socle it writes.
    pub version: &'static str,
    /// Files the socle owns, at their path in the repository, besides the
    /// [`SHARED`] ones.
    pub files: &'static Dir<'static>,
    /// Blocks, one template file each.
    pub blocks: &'static Dir<'static>,
    /// The skeleton of the project, written once by `init`.
    pub project: &'static Dir<'static>,
    /// Socle files laid out by code from the repository at the given root,
    /// which already holds the project's own files. Written, recorded and
    /// rewritten like `files`.
    pub laid_out: fn(&Path) -> Result<Vec<Template>>,
    /// Block templates: template file in `blocks`, target file in the
    /// repository, markers in the target's comment syntax.
    pub block_targets: &'static [(&'static str, &'static str, Markers)],
    /// Markers of a Markdown file.
    pub html: Markers,
    /// Markers of a file whose comments start with `#`.
    pub hash: Markers,
}

/// A file to write: path relative to the repository root with `/`
/// separators, and its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub path: String,
    pub contents: Cow<'static, [u8]>,
}

/// A block to write in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub target: &'static str,
    pub markers: Markers,
    pub content: String,
}

impl Socle {
    /// Path of the socle settings, relative to the repository root.
    pub fn config_path(&self) -> String {
        format!("{}/socle.toml", self.dir)
    }

    /// Path of the registry, relative to the repository root.
    pub fn lock_path(&self) -> String {
        format!("{}/socle.lock", self.dir)
    }

    /// Version of this CLI, which is also the version of the socle it writes.
    pub fn cli_version(&self) -> Version {
        Version::parse(self.version).expect("the crate version is semver")
    }

    /// Every file of the socle, its own and the shared ones, sorted by path.
    /// A socle's own file never takes the path of a shared one: the registry
    /// would hold that path twice.
    pub fn socle_files(&self) -> Vec<Template> {
        let mut files = files_of(self.files);
        files.extend(files_of(&SHARED));
        files.sort_by(|a, b| a.path.cmp(&b.path));
        if let Some(pair) = files.windows(2).find(|w| w[0].path == w[1].path) {
            panic!(
                "{} is both a file of the {} socle and a shared one",
                pair[0].path, self.bin
            );
        }
        files
    }

    /// Every socle file of the repository at `root`: [`Socle::socle_files`]
    /// and those laid out by code, sorted by path. A file laid out by code
    /// never takes the path of a template.
    pub fn files_in(&self, root: &Path) -> Result<Vec<Template>> {
        let mut files = self.socle_files();
        files.extend((self.laid_out)(root)?);
        files.sort_by(|a, b| a.path.cmp(&b.path));
        if let Some(pair) = files.windows(2).find(|w| w[0].path == w[1].path) {
            panic!(
                "{} is both a template and laid out by code in the {} socle",
                pair[0].path, self.bin
            );
        }
        Ok(files)
    }

    /// Every file of the project skeleton, sorted by path.
    pub fn project_files(&self) -> Vec<Template> {
        files_of(self.project)
    }

    /// Every block of the socle.
    pub fn socle_blocks(&self) -> Vec<Block> {
        self.block_targets
            .iter()
            .map(|(template, target, markers)| {
                let file = self
                    .blocks
                    .get_file(template)
                    .unwrap_or_else(|| panic!("block template {template} is embedded"));
                let text = file
                    .contents_utf8()
                    .unwrap_or_else(|| panic!("block template {template} is UTF-8"));
                Block {
                    target,
                    markers: *markers,
                    content: blocks::with_final_newline(text),
                }
            })
            .collect()
    }

    /// Checks the repository's socle version against the CLI's, at the start
    /// of every command that works in a repository: a newer socle is refused,
    /// an older one gives a warning, once per process, that suggests `update`.
    pub fn check_version<I: Serialize + DeserializeOwned>(&self, root: &Path) -> Result<()> {
        static WARNED: Once = Once::new();
        let config = SocleConfig::<I>::load(self, root)?;
        if let Some(warning) =
            compare_versions(self, &config.version, &self.cli_version())?.enforce(self)?
        {
            WARNED.call_once(|| tracing::warn!("{warning}"));
        }
        Ok(())
    }

    /// Lays the socle down in `root`, which must not hold the socle directory
    /// yet: project skeleton, socle files and blocks, registry, `CLAUDE.md`
    /// pointing at `AGENTS.md`, and the socle settings with `init`.
    ///
    /// Project files that already exist are kept as they are: the socle never
    /// overwrites the project's content.
    pub fn init<I: Serialize + DeserializeOwned>(&self, root: &Path, init: I) -> Result<()> {
        for template in self.project_files() {
            let path = under(root, &template.path);
            if !path.exists() {
                write(&path, &template.contents)?;
            }
        }
        self.write_socle(root)?.save(self, root)?;
        link_claude_md(root)?;
        SocleConfig {
            version: self.cli_version().to_string(),
            init,
        }
        .save(self, root)
    }

    /// Writes every socle file and block, and returns the registry of what
    /// was written.
    pub fn write_socle(&self, root: &Path) -> Result<SocleLock> {
        let mut lock = SocleLock::default();
        for template in self.files_in(root)? {
            write(&under(root, &template.path), &template.contents)?;
            lock.files.push(LockedFile {
                sha256: sha256_bytes(&template.contents),
                path: template.path,
                kind: LockedKind::File,
            });
        }
        for block in self.socle_blocks() {
            let path = under(root, block.target);
            let text = read_if_exists(&path)?.unwrap_or_default();
            write(
                &path,
                blocks::upsert(&text, block.markers, &block.content).as_bytes(),
            )?;
            lock.files.push(LockedFile {
                path: block.target.to_owned(),
                sha256: sha256_bytes(block.content.as_bytes()),
                kind: LockedKind::Block,
            });
        }
        Ok(lock)
    }

    /// Registry entries whose file or block no longer matches the recorded
    /// hash: edited by hand, deleted, or with a marker gone.
    pub fn edited_by_hand(&self, root: &Path, lock: &SocleLock) -> Result<Vec<String>> {
        let mut edited = Vec::new();
        for entry in &lock.files {
            let path = under(root, &entry.path);
            let current = match entry.kind {
                LockedKind::File if path.is_file() => {
                    Some(sha256_file(&path).context(format!("reading {}", path.display()))?)
                }
                LockedKind::File => None,
                LockedKind::Block => read_if_exists(&path)?.and_then(|text| {
                    blocks::find(&text, self.markers_of(&entry.path))
                        .map(|c| sha256_bytes(c.as_bytes()))
                }),
            };
            if current.as_deref() != Some(entry.sha256.as_str()) {
                edited.push(entry.path.clone());
            }
        }
        Ok(edited)
    }

    /// Removes what `old` recorded and `new` no longer has: socle files that
    /// left the templates, and blocks whose template is gone. Returns their
    /// paths.
    pub fn remove_stale(
        &self,
        root: &Path,
        old: &SocleLock,
        new: &SocleLock,
    ) -> Result<Vec<String>> {
        let mut removed = Vec::new();
        for entry in &old.files {
            let still = new
                .files
                .iter()
                .any(|n| n.path == entry.path && n.kind == entry.kind);
            if still {
                continue;
            }
            let path = under(root, &entry.path);
            match entry.kind {
                LockedKind::File => {
                    if path.is_file() {
                        fs::remove_file(&path).context(format!("removing {}", path.display()))?;
                    }
                    remove_empty_parents(root, &path);
                }
                LockedKind::Block => {
                    if let Some(text) = read_if_exists(&path)? {
                        write(
                            &path,
                            blocks::remove(&text, self.markers_of(&entry.path)).as_bytes(),
                        )?;
                    }
                }
            }
            removed.push(entry.path.clone());
        }
        Ok(removed)
    }

    /// `update`: brings the repository's socle at `root` to this CLI's
    /// version, and prints what it did.
    ///
    /// 1. Compares the repository's socle version to the CLI's; stops when
    ///    equal, unless `force`.
    /// 2. Checks every file and block of the registry against its recorded
    ///    hash; stops on hand edits unless `force`.
    /// 3. Writes the socle files and blocks, removes those that left the
    ///    socle.
    /// 4. Rewrites the registry and the version.
    pub fn update<I: Serialize + DeserializeOwned>(&self, root: &Path, force: bool) -> Result<()> {
        let mut config = SocleConfig::<I>::load(self, root)?;
        let cli = self.cli_version();
        let check = compare_versions(self, &config.version, &cli)?;
        match check {
            // `force` rewrites the socle at the same version: a file deleted
            // or edited by hand, or a broken registry.
            VersionCheck::Equal if !force => {
                println!("The socle is already at version {cli}: nothing to update.");
                return Ok(());
            }
            VersionCheck::Equal => {}
            VersionCheck::RepoNewer { .. } => {
                check.enforce(self)?;
            }
            VersionCheck::RepoOlder { .. } => {}
        }

        let old = match SocleLock::load(self, root) {
            Ok(lock) => lock,
            Err(err) if force => {
                tracing::warn!("ignoring the unreadable socle registry: {err}");
                SocleLock::default()
            }
            Err(err) => return Err(err),
        };
        let edited = self.edited_by_hand(root, &old)?;
        if !edited.is_empty() {
            if !force {
                return Err(Error::blocked(
                    format!(
                        "socle files edited by hand since the socle wrote them:{}",
                        list(&edited)
                    ),
                    format!(
                        "move your changes out of these files, then rerun `{} update --force` to overwrite them",
                        self.bin
                    ),
                ));
            }
            tracing::warn!("overwriting socle files edited by hand:{}", list(&edited));
        }

        let new = self.write_socle(root)?;
        let removed = self.remove_stale(root, &old, &new)?;
        new.save(self, root)?;
        // A socle that writes `AGENTS.md` for the first time links it as
        // `init` does; a link deleted afterwards stays deleted.
        if old.files.iter().all(|f| f.path != AGENTS_MD) {
            link_claude_md(root)?;
        }
        let from = std::mem::replace(&mut config.version, cli.to_string());
        config.save(self, root)?;

        println!("Updated the socle from {from} to {cli}.");
        for path in removed {
            println!("Removed {path}, no longer part of the socle.");
        }
        Ok(())
    }

    /// Markers of a block's target file: the known targets first, then by
    /// the file's extension.
    fn markers_of(&self, target: &str) -> Markers {
        self.block_targets
            .iter()
            .find(|(_, t, _)| *t == target)
            .map(|(_, _, m)| *m)
            .unwrap_or(if target.ends_with(".md") {
                self.html
            } else {
                self.hash
            })
    }
}

fn list(paths: &[String]) -> String {
    paths.iter().map(|p| format!("\n  - {p}")).collect()
}

fn files_of(dir: &'static Dir<'static>) -> Vec<Template> {
    fn walk(dir: &'static Dir<'static>, out: &mut Vec<Template>) {
        for file in dir.files() {
            let path = file
                .path()
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.push(Template {
                path,
                contents: Cow::Borrowed(file.contents()),
            });
        }
        for sub in dir.dirs() {
            walk(sub, out);
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// A path of the registry or of a template, under the repository root.
fn under(root: &Path, path: &str) -> PathBuf {
    path.split('/').fold(root.to_path_buf(), |p, c| p.join(c))
}

/// Removes the directories left empty between `file` and `root`.
fn remove_empty_parents(root: &Path, file: &Path) {
    for dir in file.ancestors().skip(1) {
        if dir == root || !dir.starts_with(root) || fs::remove_dir(dir).is_err() {
            break;
        }
    }
}

fn read_if_exists(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context(format!("reading {}", path.display())),
    }
}

/// Writes a file, creating its directory. A template starting with `#!` is a
/// script (a git hook): it is made executable where that means something.
fn write(path: &Path, contents: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).context(format!("creating {}", dir.display()))?;
    }
    fs::write(path, contents).context(format!("writing {}", path.display()))?;
    #[cfg(unix)]
    if contents.starts_with(b"#!") {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .context(format!("making {} executable", path.display()))?;
    }
    Ok(())
}

/// Points `CLAUDE.md` at `AGENTS.md`, unless something named `CLAUDE.md`
/// already exists or there is no `AGENTS.md` to point at. A `CLAUDE.md` of
/// its own is kept, with a warning.
fn link_claude_md(root: &Path) -> Result<()> {
    let link = root.join(CLAUDE_MD);
    if !root.join(AGENTS_MD).exists() {
        return Ok(());
    }
    if let Ok(meta) = link.symlink_metadata() {
        // A file that only imports AGENTS.md is as good as the link.
        let imports =
            std::fs::read_to_string(&link).is_ok_and(|s| s.trim() == format!("@{AGENTS_MD}"));
        if meta.is_file() && !imports {
            tracing::warn!(
                "{CLAUDE_MD} is not a link to {AGENTS_MD}: agents reading one miss the other. Move its content into {AGENTS_MD}, then replace it with a link to {AGENTS_MD}"
            );
        }
        return Ok(());
    }
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(AGENTS_MD, &link);
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(AGENTS_MD, &link);
    #[cfg(not(any(unix, windows)))]
    let made: std::io::Result<()> = Err(std::io::ErrorKind::Unsupported.into());
    if let Err(e) = made {
        tracing::warn!("could not link {CLAUDE_MD} to {AGENTS_MD}: {e}; create the link yourself");
    }
    Ok(())
}

/// A socle without templates, for the tests of the mechanics.
#[cfg(test)]
pub(crate) static TEST: Socle = {
    static EMPTY: Dir<'static> = Dir::new("", &[]);
    Socle {
        dir: ".test",
        bin: "test",
        tool: "test-cli",
        version: "0.3.0",
        files: &EMPTY,
        blocks: &EMPTY,
        project: &EMPTY,
        laid_out: |_| Ok(Vec::new()),
        block_targets: &[],
        html: Markers {
            begin: "<!-- test:begin -->",
            end: "<!-- test:end -->",
        },
        hash: Markers {
            begin: "# test:begin",
            end: "# test:end",
        },
    }
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A socle with no file of its own still lays down the shared ones, and
    /// a shared skill is a Claude Code skill named after its directory.
    #[test]
    fn every_socle_owns_the_shared_skills() {
        let files = TEST.socle_files();
        let skill = files
            .iter()
            .find(|t| t.path == ".claude/skills/mc-curseforge-page/SKILL.md")
            .expect("the CurseForge page skill is shared");
        let text = std::str::from_utf8(&skill.contents).unwrap();
        assert!(text.starts_with("---\nname: mc-curseforge-page\ndescription: "));
        assert!(text.contains("Use when"));
        assert!(!text.contains('\u{2014}'));
        for file in &files {
            if let Some(rest) = file.path.strip_prefix(".claude/skills/") {
                assert!(
                    rest.starts_with("mc-"),
                    "{}: shared skills are prefixed `mc-`",
                    file.path
                );
            }
        }
    }

    #[test]
    #[should_panic(
        expected = ".claude/skills/mc-curseforge-page/SKILL.md is both a file of the test socle and a shared one"
    )]
    fn a_socle_file_never_shadows_a_shared_one() {
        static FILES: Dir<'static> = Dir::new(
            "",
            &[include_dir::DirEntry::File(include_dir::File::new(
                ".claude/skills/mc-curseforge-page/SKILL.md",
                b"its own",
            ))],
        );
        let socle = Socle {
            files: &FILES,
            ..TEST
        };
        socle.socle_files();
    }

    #[test]
    fn claude_md_is_linked_only_to_an_existing_agents_md() {
        let dir = tempfile::tempdir().unwrap();
        link_claude_md(dir.path()).unwrap();
        assert!(dir.path().join(CLAUDE_MD).symlink_metadata().is_err());
        fs::write(dir.path().join(AGENTS_MD), "# Agents\n").unwrap();
        link_claude_md(dir.path()).unwrap();
        #[cfg(unix)]
        assert_eq!(
            fs::read_link(dir.path().join(CLAUDE_MD)).unwrap(),
            Path::new(AGENTS_MD)
        );
    }

    /// A socle that had no `AGENTS.md` block gets one in a later version: the
    /// update that writes it links `CLAUDE.md`, as `init` would have. Later
    /// updates leave a deleted link deleted.
    #[cfg(unix)]
    #[test]
    fn update_links_claude_md_when_agents_md_arrives() {
        use include_dir::{DirEntry, File};
        type Init = std::collections::BTreeMap<String, String>;
        static AGENTS: Dir<'static> = Dir::new(
            "",
            &[DirEntry::File(File::new(
                "agents.md",
                b"Read the design.\n",
            ))],
        );
        let with_agents = |version| Socle {
            version,
            blocks: &AGENTS,
            block_targets: &[(
                "agents.md",
                AGENTS_MD,
                Markers {
                    begin: "<!-- test:begin -->",
                    end: "<!-- test:end -->",
                },
            )],
            ..TEST
        };
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join(CLAUDE_MD);
        TEST.init(dir.path(), Init::new()).unwrap();
        assert!(link.symlink_metadata().is_err());

        with_agents("0.4.0")
            .update::<Init>(dir.path(), false)
            .unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), Path::new(AGENTS_MD));

        fs::remove_file(&link).unwrap();
        with_agents("0.5.0")
            .update::<Init>(dir.path(), false)
            .unwrap();
        assert!(link.symlink_metadata().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn scripts_are_written_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hook");
        write(&path, b"#!/bin/sh\n").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
    }
}
