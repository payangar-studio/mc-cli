//! The `design` branch of a mod: an orphan branch that does not follow the
//! code, with the design as Markdown documents for agents (`docs/design/*.md`)
//! and as a wiki of self-contained pages for people, whose home is
//! `docs/design/index.html`, which GitHub Pages serves from `/docs`. `docs/.nojekyll` makes Pages serve the
//! files as they are.
//!
//! `mod init` writes the branch with git plumbing (blobs, trees, one commit,
//! the ref), so the working tree, the index and the current branch are left
//! alone. It stays local: `mod setup` pushes it and sets Pages once the
//! version branch is on GitHub ([`pages`]). The `mod-wiki-design` skill builds the
//! pages from the documents.

pub mod pages;

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{Error, Result};
use crate::model::mod_toml::ModToml;
use crate::socle::skeleton::{File, fill};

/// Name of the branch.
pub const BRANCH: &str = "design";

/// The page GitHub Pages serves, from the root of the branch.
pub const PAGE: &str = "docs/design/index.html";

/// The files of a new `design` branch for `module`.
pub fn files(module: &ModToml) -> Vec<File> {
    let values = [
        ("name", module.name.clone()),
        ("name_html", html_escape(&module.name)),
        ("id", module.id.clone()),
        ("side", module.side.to_string()),
    ];
    vec![
        File {
            path: "docs/.nojekyll".into(),
            contents: Vec::new(),
        },
        File {
            path: PAGE.into(),
            contents: fill(include_str!("../../templates/design/index.html"), &values).into(),
        },
        File {
            path: "docs/design/00-overview.md".into(),
            contents: fill(include_str!("../../templates/design/overview.md"), &values).into(),
        },
    ]
}

/// What [`create_branch`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Branch {
    Created,
    /// A `design` branch exists already, locally or on a remote: kept.
    Exists,
    /// Local branches under `design/` (named here) take the name: git cannot
    /// create `design` beside them.
    Clash(Vec<String>),
}

/// Creates the orphan `design` branch of `module` in the repository `root`,
/// unless one exists locally or on a remote. Needs a git identity to commit.
pub fn create_branch(root: &Path, module: &ModToml) -> Result<Branch> {
    let refs = git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
        None,
    )?;
    // A pattern of for-each-ref also matches the refs under it
    // (`refs/heads/design/ideas`): the names are compared whole.
    let exists = refs.lines().any(|r| {
        r.strip_prefix("refs/heads/") == Some(BRANCH)
            || r.strip_prefix("refs/remotes/")
                .and_then(|r| r.split_once('/'))
                .is_some_and(|(_, branch)| branch == BRANCH)
    });
    if exists {
        return Ok(Branch::Exists);
    }
    let clash: Vec<String> = refs
        .lines()
        .filter_map(|r| r.strip_prefix("refs/heads/"))
        .filter(|r| r.strip_prefix(BRANCH).is_some_and(|r| r.starts_with('/')))
        .map(str::to_owned)
        .collect();
    if !clash.is_empty() {
        return Ok(Branch::Clash(clash));
    }
    let tree = write_tree(root, &files(module))?;
    let message = format!("docs: start the design of {}", module.name);
    let commit = run(root, &["commit-tree", &tree, "-m", &message], None)?;
    if !commit.status.success() {
        return Err(Error::internal(
            format!(
                "could not commit the `{BRANCH}` branch: {}",
                String::from_utf8_lossy(&commit.stderr).trim()
            ),
            format!(
                "set your git identity (`git config --global user.name ...` and `user.email`), then rerun `mod init` in {}",
                root.display()
            ),
        ));
    }
    let commit = String::from_utf8_lossy(&commit.stdout).trim().to_owned();
    // The empty old value refuses to overwrite a branch created meanwhile.
    git(
        root,
        &["update-ref", &format!("refs/heads/{BRANCH}"), &commit, ""],
        None,
    )?;
    Ok(Branch::Created)
}

/// A directory of the tree being written: file blobs and subdirectories.
#[derive(Default)]
struct Dir {
    files: BTreeMap<String, String>,
    dirs: BTreeMap<String, Dir>,
}

/// Writes `files` as git objects and returns the id of their root tree.
fn write_tree(root: &Path, files: &[File]) -> Result<String> {
    let mut top = Dir::default();
    for file in files {
        let blob = git(
            root,
            &["hash-object", "-w", "--stdin"],
            Some(&file.contents),
        )?;
        let mut parts: Vec<&str> = file.path.split('/').collect();
        let name = parts.pop().expect("a path has a file name");
        let dir = parts.into_iter().fold(&mut top, |dir, part| {
            dir.dirs.entry(part.into()).or_default()
        });
        dir.files.insert(name.into(), blob);
    }
    make_tree(root, &top)
}

fn make_tree(root: &Path, dir: &Dir) -> Result<String> {
    let mut listing = String::new();
    for (name, blob) in &dir.files {
        listing.push_str(&format!("100644 blob {blob}\t{name}\n"));
    }
    for (name, sub) in &dir.dirs {
        let tree = make_tree(root, sub)?;
        listing.push_str(&format!("040000 tree {tree}\t{name}\n"));
    }
    git(root, &["mktree"], Some(listing.as_bytes()))
}

/// Runs git in `root` with `input` on stdin; returns its trimmed stdout.
fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> Result<String> {
    let output = run(root, args, input)?;
    if !output.status.success() {
        return Err(Error::internal(
            format!(
                "`git {}` failed: {}",
                args[0],
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            GIT_HINT,
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

const GIT_HINT: &str = "install git, or check the repository with `git status`";

fn run(root: &Path, args: &[&str], input: Option<&[u8]>) -> Result<std::process::Output> {
    let failed = |e: std::io::Error| Error::internal("could not run git", GIT_HINT).with_source(e);
    let mut child = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(failed)?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(input).map_err(failed)?;
    }
    child.wait_with_output().map_err(failed)
}

/// `value` as HTML text.
fn html_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mod_toml::{Loader, Side};

    fn text(bytes: &[u8]) -> &str {
        std::str::from_utf8(bytes).expect("the design files are text")
    }

    fn module(name: &str) -> ModToml {
        ModToml {
            id: "swift_blades".into(),
            name: name.into(),
            package: "com.payangar.swiftblades".into(),
            side: Side::BothOptional,
            loaders: vec![Loader::Fabric],
            license: "MIT".into(),
            dependencies: vec![],
        }
    }

    #[test]
    fn the_branch_holds_the_page_the_documents_and_nojekyll() {
        let files = files(&module("Swift Blades"));
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "docs/.nojekyll",
                "docs/design/index.html",
                "docs/design/00-overview.md"
            ]
        );
        for file in &files {
            assert!(
                !text(&file.contents).contains("{{"),
                "{} keeps a placeholder",
                file.path
            );
        }
        let overview = text(&files[2].contents);
        assert!(overview.starts_with("# Swift Blades: design\n"));
        assert!(overview.contains("`both-optional`"));
    }

    #[test]
    fn the_name_is_escaped_in_the_page() {
        let files = files(&module("<Blades> & \"Co\""));
        let page = text(&files[1].contents);
        assert!(page.contains("<h1>&lt;Blades&gt; &amp; &quot;Co&quot;</h1>"));
        assert!(!page.contains("<Blades>"));
    }
}
