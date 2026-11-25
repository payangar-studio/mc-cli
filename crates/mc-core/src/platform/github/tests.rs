//! The GitHub platform against a mocked API.

use std::io::{Cursor, Read};

use mockito::{Mock, Server, ServerGuard};
use serde_json::json;

use super::*;
use crate::hash::copy_hashing;

fn pack(loader: &str) -> Pack {
    Pack {
        minecraft: "26.3".into(),
        loader: loader.into(),
        loader_version: "1".into(),
    }
}

struct Api {
    server: ServerGuard,
    cache: tempfile::TempDir,
    mocks: Vec<Mock>,
}

impl Api {
    fn new() -> Self {
        Self {
            server: Server::new(),
            cache: tempfile::tempdir().unwrap(),
            mocks: Vec::new(),
        }
    }

    fn github(&self) -> GitHub {
        GitHub::new(&self.server.url(), self.cache.path().to_path_buf())
    }

    fn asset(&self, name: &str) -> serde_json::Value {
        let bytes = name.as_bytes();
        let hashes = copy_hashing(&mut Cursor::new(bytes), &mut std::io::sink()).unwrap();
        json!({
            "name": name,
            "size": bytes.len(),
            "browser_download_url": format!("{}/download/{}", self.server.url(), encode_segment(name)),
            "digest": format!("sha256:{}", hashes.sha256),
        })
    }

    fn release(&self, tag: &str, prerelease: bool, files: &[&str]) -> serde_json::Value {
        json!({
            "tag_name": tag,
            "name": tag,
            "draft": false,
            "prerelease": prerelease,
            "assets": files.iter().map(|f| self.asset(f)).collect::<Vec<_>>(),
        })
    }

    /// Serves `releases` of `repo`, as the list and one by one by tag.
    fn repo(&mut self, repo: &str, releases: &[serde_json::Value]) {
        let mock = self
            .server
            .mock(
                "GET",
                format!("/repos/{repo}/releases?per_page=100").as_str(),
            )
            .with_body(serde_json::Value::Array(releases.to_vec()).to_string())
            .create();
        self.mocks.push(mock);
        for release in releases {
            let tag = encode_segment(release["tag_name"].as_str().unwrap());
            let mock = self
                .server
                .mock("GET", format!("/repos/{repo}/releases/tags/{tag}").as_str())
                .with_body(release.to_string())
                .create();
            self.mocks.push(mock);
        }
    }

    /// Serves the file `name` (its bytes are its name), expecting `hits`
    /// downloads.
    fn file(&mut self, name: &str, hits: usize) -> Mock {
        // GitHub's URLs encode a `+` of the name as `%2B`.
        let path = format!(
            "^/download/{}$",
            name.replace('.', r"\.").replace('+', "(\\+|%2B)")
        );
        self.server
            .mock("GET", mockito::Matcher::Regex(path))
            .with_body(name)
            .expect(hits)
            .create()
    }
}

/// Lithium's release layout: a license, an API jar and one jar per loader.
const LITHIUM: [&str; 4] = [
    "LICENSE.md",
    "lithium-0.26.4+mc26.3-api.jar",
    "lithium-fabric-0.26.4+mc26.3.jar",
    "lithium-neoforge-0.26.4+mc26.3.jar",
];

#[test]
fn queries_name_a_repository_a_tag_and_a_file() {
    let q = |s: &str| Query::parse(s).unwrap().unwrap();
    assert_eq!(
        q("github:CaffeineMC/lithium"),
        Query {
            repo: "CaffeineMC/lithium".into(),
            tag: None,
            file: None
        }
    );
    assert_eq!(q("github:a/b@v1").tag.as_deref(), Some("v1"));
    let named = q("github:a/b@v1/b-fabric.jar");
    assert_eq!(
        (named.tag.as_deref(), named.file.as_deref()),
        (Some("v1"), Some("b-fabric.jar"))
    );
    // A tag may hold a `/`: only a file name ends it.
    assert_eq!(
        q("github:a/b@release/1.0").tag.as_deref(),
        Some("release/1.0")
    );

    assert_eq!(Query::parse("sodium").unwrap(), None);
    for bad in [
        "github:a",
        "github:a/b/c",
        "github:/b",
        "github:a/b@",
        "github:a b/c",
    ] {
        let err = Query::parse(bad).unwrap_err();
        assert!(
            err.to_string().contains("is not a GitHub repository"),
            "{bad}: {err}"
        );
    }
}

#[test]
fn a_release_offers_the_jar_of_the_loader() {
    let mut api = Api::new();
    let release = api.release("mc26.3-0.26.4", false, &LITHIUM);
    api.repo("CaffeineMC/lithium", &[release]);
    let github = api.github();
    for (loader, file) in [
        ("fabric", "lithium-fabric-0.26.4+mc26.3.jar"),
        ("neoforge", "lithium-neoforge-0.26.4+mc26.3.jar"),
    ] {
        let candidates = github
            .resolve("github:CaffeineMC/lithium@mc26.3-0.26.4", &pack(loader))
            .unwrap();
        assert_eq!(candidates.len(), 1);
        let c = &candidates[0];
        assert_eq!(
            (c.label.as_str(), c.file_name.as_str(), c.kind),
            ("mc26.3-0.26.4", file, Kind::Mod)
        );
        assert_eq!(c.slug.as_deref(), Some("lithium"));
        assert_eq!(c.section["repo"].as_str(), Some("CaffeineMC/lithium"));
        assert_eq!(c.section["file"].as_str(), Some(file));
    }
}

#[test]
fn several_candidates_are_listed_until_one_is_named() {
    let mut api = Api::new();
    let release = api.release(
        "v1",
        false,
        &["a-1.0.jar", "a-1.0-extra.jar", "a-1.0-sources.jar"],
    );
    api.repo("o/a", &[release]);
    let github = api.github();

    let err = github
        .resolve("github:o/a@v1", &pack("fabric"))
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("several files that could be meant: a-1.0.jar, a-1.0-extra.jar"),
        "{err}"
    );
    assert_eq!(err.hint(), "name the file: `github:o/a@v1/<file>`");

    let named = github
        .resolve("github:o/a@v1/a-1.0-extra.jar", &pack("fabric"))
        .unwrap();
    assert_eq!(named[0].file_name, "a-1.0-extra.jar");
}

/// Without a tag: the releases for the pack's Minecraft when the repository
/// names versions, releases before prereleases.
#[test]
fn without_a_tag_the_latest_fitting_release_comes_first() {
    let mut api = Api::new();
    let releases = [
        api.release("mc26.4-2.0", false, &["m-2.0+mc26.4.jar"]),
        api.release("mc26.3-1.2-beta", true, &["m-1.2+mc26.3.jar"]),
        api.release("mc26.3-1.1", false, &["m-1.1+mc26.3.jar"]),
    ];
    api.repo("o/m", &releases);
    let labels: Vec<String> = api
        .github()
        .resolve("github:o/m", &pack("fabric"))
        .unwrap()
        .into_iter()
        .map(|c| c.label)
        .collect();
    assert_eq!(labels, ["mc26.3-1.1", "mc26.3-1.2-beta"]);
}

/// Resolving then filling an entry reads its release once: the API allows
/// 60 anonymous requests an hour.
#[test]
fn a_release_is_read_once() {
    let mut api = Api::new();
    let release = api.release("v1", false, &["a.jar"]);
    let tag = api
        .server
        .mock("GET", "/repos/o/a/releases/tags/v1")
        .with_body(release.to_string())
        .expect(1)
        .create();
    let _file = api.file("a.jar", 1);
    let github = api.github();
    let mut e = entry(&github, "github:o/a@v1");
    github.fill_entry(&mut e).unwrap();
    tag.assert();
}

/// A Minecraft version is matched whole: a release for 1.21.1 is not one
/// for 1.21.
#[test]
fn a_release_names_a_minecraft_version_whole() {
    for text in [
        "mc1.21-0.5",
        "mod-1.21.jar",
        "1.21",
        "v1.21.x",
        "1.21+fabric",
    ] {
        assert!(names_version(text, "1.21"), "{text}");
    }
    for text in ["mod-1.21.1.jar", "11.21", "mc1.21.10", "0.1.21-mc26.3"] {
        assert!(!names_version(text, "1.21"), "{text}");
    }
}

/// An entry's table names a path under the cache: one that leaves it is
/// refused.
#[test]
fn a_table_cannot_name_a_path_outside_the_cache() {
    let api = Api::new();
    let github = api.github();
    for (repo, file) in [
        ("o/a", "../../x.jar"),
        ("o/a", "d/x.jar"),
        ("../a", "x.jar"),
    ] {
        let mut table = toml::Table::new();
        table.insert("repo".into(), repo.into());
        table.insert("tag".into(), "v1".into());
        table.insert("file".into(), file.into());
        let entry = Entry {
            key: "x".into(),
            kind: Kind::Mod,
            path: "mods/x.jar".into(),
            file_size: 1,
            downloads: vec!["https://example.invalid/x.jar".into()],
            url: None,
            source: None,
            approved: None,
            hashes: Hashes {
                sha1: String::new(),
                sha512: String::new(),
            },
            env: Kind::Mod.default_env(),
            platforms: [(NAME.to_owned(), table)].into(),
        };
        let Err(err) = github.download(&entry) else {
            panic!("{repo} {file}: refused");
        };
        assert!(
            err.to_string().contains("not a release file"),
            "{repo} {file}: {err}"
        );
    }
}

#[test]
fn a_resource_pack_is_a_zip_of_a_release_without_jar() {
    let mut api = Api::new();
    let release = api.release("v3", false, &["pack-v3.zip"]);
    api.repo("o/pack", &[release]);
    let candidates = api
        .github()
        .resolve("github:o/pack@v3", &pack("fabric"))
        .unwrap();
    assert_eq!(candidates[0].kind, Kind::Resourcepack);
}

#[test]
fn a_release_without_files_or_an_unknown_tag_are_usage_errors() {
    let mut api = Api::new();
    let release = api.release("v1", false, &["LICENSE.md"]);
    api.repo("o/a", &[release]);
    let _missing = api
        .server
        .mock("GET", "/repos/o/a/releases/tags/v9")
        .with_status(404)
        .create();
    let github = api.github();
    let err = github
        .resolve("github:o/a@v1", &pack("fabric"))
        .unwrap_err();
    assert!(
        err.to_string().contains("has no .jar nor .zip file"),
        "{err}"
    );
    let err = github
        .resolve("github:o/a@v9", &pack("fabric"))
        .unwrap_err();
    assert!(
        err.to_string().contains("no release `v9` of `o/a`"),
        "{err}"
    );
    // Another platform's query is not GitHub's.
    assert!(
        github
            .resolve("sodium", &pack("fabric"))
            .unwrap()
            .is_empty()
    );
}

fn entry(github: &GitHub, query: &str) -> Entry {
    let c = github.resolve(query, &pack("fabric")).unwrap().remove(0);
    Entry {
        key: c.slug.clone().unwrap(),
        kind: c.kind,
        path: format!("{}/{}", c.kind.dir(), c.file_name),
        file_size: 0,
        downloads: Vec::new(),
        url: None,
        source: None,
        approved: None,
        hashes: Hashes {
            sha1: String::new(),
            sha512: String::new(),
        },
        env: c.kind.default_env(),
        platforms: [(NAME.to_owned(), c.section)].into(),
    }
}

/// A machine downloads a file once: filling the entry caches it, and both a
/// second fill and the download read the cached copy.
#[test]
fn a_file_is_downloaded_once_per_machine() {
    let mut api = Api::new();
    let release = api.release("mc26.3-0.26.4", false, &LITHIUM);
    api.repo("CaffeineMC/lithium", &[release]);
    let jar = "lithium-fabric-0.26.4+mc26.3.jar";
    let file = api.file(jar, 1);
    let github = api.github();
    let query = "github:CaffeineMC/lithium@mc26.3-0.26.4";

    let mut first = entry(&github, query);
    github.fill_entry(&mut first).unwrap();
    let expected = copy_hashing(&mut Cursor::new(jar.as_bytes()), &mut std::io::sink()).unwrap();
    assert!(expected.matches(&first.hashes));
    assert_eq!(first.file_size, jar.len() as u64);
    assert_eq!(first.downloads.len(), 1);

    let mut second = entry(&github, query);
    github.fill_entry(&mut second).unwrap();
    assert_eq!(second.hashes, first.hashes);
    let Download::Stream(mut stream) = github.download(&second).unwrap() else {
        panic!("the cached copy is streamed");
    };
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, jar.as_bytes());
    file.assert();
}

#[test]
fn a_download_that_does_not_match_the_digest_is_refused() {
    let mut api = Api::new();
    let mut release = api.release("v1", false, &["a.jar"]);
    release["assets"][0]["digest"] = json!(format!("sha256:{}", "0".repeat(64)));
    api.repo("o/a", &[release]);
    let _file = api.file("a.jar", 1);
    let github = api.github();
    let mut e = entry(&github, "github:o/a@v1");
    let err = github.fill_entry(&mut e).unwrap_err();
    assert!(
        err.to_string().contains("does not match the sha256"),
        "{err}"
    );
    assert!(!api.cache.path().join("o/a/v1/a.jar").exists());
}

#[test]
fn the_rate_limit_says_when_it_resets() {
    let mut api = Api::new();
    let reset = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let _limited = api
        .server
        .mock("GET", "/repos/o/a/releases/tags/v1")
        .with_status(403)
        .with_header("x-ratelimit-remaining", "0")
        .with_header("x-ratelimit-reset", &reset.to_string())
        .create();
    let err = api
        .github()
        .resolve("github:o/a@v1", &pack("fabric"))
        .unwrap_err();
    assert!(err.to_string().contains("rate limit"), "{err}");
    assert!(
        err.hint().contains("resets in 10 minutes"),
        "{}",
        err.hint()
    );
}

#[test]
fn github_identifies_no_jar() {
    let api = Api::new();
    let jar = Jar {
        path: "mods/a.jar".into(),
        sha1: "1".into(),
        sha512: "5".into(),
        size: 1,
    };
    assert_eq!(api.github().identify(&[jar]).unwrap().len(), 1);
}
