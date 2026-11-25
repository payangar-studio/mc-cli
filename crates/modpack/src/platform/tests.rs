//! The modpack operations of each platform.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::fake::{FakeMod, FakePlatform};
use super::*;
use crate::model::manifest::{Env, Hashes, Kind, Manifest, Support};

fn pack() -> Pack {
    Pack {
        minecraft: "1.21.1".into(),
        loader: "fabric".into(),
        loader_version: "0.16.0".into(),
    }
}

/// An entry of `kind` mod with the platform table `table` under `platform`.
fn entry(platform: &str, table: toml::Table) -> Entry {
    Entry {
        key: "jei".into(),
        kind: Kind::Mod,
        path: "mods/jei.jar".into(),
        file_size: 1,
        downloads: Vec::new(),
        url: None,
        source: None,
        approved: None,
        hashes: Hashes {
            sha1: String::new(),
            sha512: String::new(),
        },
        env: Env::default(),
        platforms: BTreeMap::from([(platform.to_owned(), table)]),
    }
}

fn fake() -> FakePlatform {
    let mut sodium = FakeMod::new("sodium");
    sodium.env.server = Support::Unsupported;
    sodium.dependencies = vec!["fabric-api".into()];
    FakePlatform::new("fake")
        .with(sodium)
        .with(FakeMod::new("fabric-api"))
}

fn fake_entry(p: &FakePlatform, slug: &str) -> Entry {
    let candidate = p.resolve(slug, &pack()).unwrap().remove(0);
    let mut entry = entry("fake", candidate.section);
    entry.key = slug.to_owned();
    entry.path = format!("{}/{}", candidate.kind.dir(), candidate.file_name);
    entry
}

#[test]
fn entries_written_by_the_fake_survive_the_manifest() {
    let p = fake();
    let mut entry = fake_entry(&p, "sodium");
    p.fill_entry(&mut entry).unwrap();
    let mut manifest = Manifest::new(pack());
    manifest.upsert(entry);
    assert_eq!(Manifest::parse(&manifest.to_toml()).unwrap(), manifest);
}

#[test]
fn fake_eligibility_and_pack() {
    let p = fake();
    let hosted = fake_entry(&p, "sodium");
    let mut foreign = hosted.clone();
    foreign.key = "other".into();
    foreign.platforms.clear();
    assert_eq!(p.pack_eligibility(&hosted).unwrap(), Eligibility::Eligible);
    assert!(matches!(
        p.pack_eligibility(&foreign).unwrap(),
        Eligibility::Blocked { .. }
    ));

    let dir = tempfile::tempdir().unwrap();
    let pack = pack();
    let path = p
        .build_pack(&PackBuild {
            name: "Test",
            version: "1.0.0",
            pack: &pack,
            entries: vec![&hosted],
            overrides: Vec::new(),
            out_dir: dir.path(),
            file_stem: "test-default-1.0.0",
        })
        .unwrap();
    assert_eq!(path, dir.path().join("test-default-1.0.0.fake"));
    assert!(fs::read_to_string(path).unwrap().contains("mod sodium"));
}

/// A platform that implements nothing.
struct Bare;

impl Platform for Bare {
    fn name(&self) -> &str {
        "bare"
    }
    fn capability(&self, _: Operation) -> Capability {
        Capability::Unsupported
    }
}

impl PackPlatform for Bare {}

#[test]
fn unimplemented_pack_operations_fail_with_a_clear_error() {
    let err = Bare.build_pack(&PackBuild {
        name: "Test",
        version: "1.0.0",
        pack: &pack(),
        entries: Vec::new(),
        overrides: Vec::new(),
        out_dir: Path::new("."),
        file_stem: "test",
    });
    assert_eq!(
        err.unwrap_err().to_string().lines().next().unwrap(),
        "error: platform `bare` does not support `build_pack`"
    );
    assert!(
        Bare.pack_eligibility(&fake_entry(&fake(), "sodium"))
            .is_err()
    );
    assert!(Bare.embedded_jar_note().is_none());
}

#[test]
fn builtin_registry_lists_github_then_modrinth_then_curseforge() {
    assert_eq!(builtin().names(), ["github", "modrinth", "curseforge"]);
}

/// A GitHub entry goes into a `.mrpack` by its release URL: GitHub builds no
/// pack itself, so `modpack pack` never offers it.
#[test]
fn github_builds_no_pack() {
    let github = builtin();
    let github = github.get("github").unwrap();
    assert_eq!(
        github.capability(Operation::BuildPack),
        Capability::Unsupported
    );
    assert_eq!(
        github.capability(Operation::PushPage),
        Capability::Unsupported
    );
}

#[test]
fn modrinth_eligibility_follows_the_download_hosts() {
    let platform = Modrinth::new("http://127.0.0.1:9");
    let mut table = toml::Table::new();
    table.insert("project_id".into(), "AANobbMI".into());
    table.insert("version_id".into(), "Pb3OXVqC".into());
    let mut entry = entry(modrinth::NAME, table);
    entry.downloads = vec!["https://cdn.modrinth.com/data/AANobbMI/x.jar".into()];
    assert_eq!(
        platform.pack_eligibility(&entry).unwrap(),
        Eligibility::Eligible
    );
    entry.downloads = vec!["https://example.org/x.jar".into()];
    let Eligibility::Blocked { reason } = platform.pack_eligibility(&entry).unwrap() else {
        panic!("a mod hosted elsewhere must be embedded");
    };
    assert!(reason.contains("embedded"));
}

/// The reason CurseForge gives for an entry without CurseForge ids.
fn curseforge_refusal(cf: &CurseForge) -> String {
    let mut e = entry(curseforge::NAME, toml::Table::new());
    e.platforms.clear();
    let Eligibility::Blocked { reason } = cf.pack_eligibility(&e).unwrap() else {
        panic!("a mod off CurseForge is not eligible");
    };
    reason
}

#[test]
fn a_mod_with_curseforge_ids_is_eligible_without_a_key() {
    let cf = CurseForge::with("http://unused.invalid", || None);
    let mut table = toml::Table::new();
    table.insert("projectID".into(), 1.into());
    table.insert("fileID".into(), 2.into());
    assert_eq!(
        cf.pack_eligibility(&entry(curseforge::NAME, table))
            .unwrap(),
        Eligibility::Eligible
    );
}

#[test]
fn a_mod_not_on_curseforge_blocks_the_pack() {
    let reason = curseforge_refusal(&CurseForge::with("http://unused.invalid", || {
        Some("test-key".into())
    }));
    assert!(reason.starts_with("not hosted on CurseForge"), "{reason}");
    assert!(reason.contains("approved list"));
}

#[test]
fn without_a_key_missing_ids_blame_the_key_not_the_hosting() {
    let reason = curseforge_refusal(&CurseForge::with("http://unused.invalid", || None));
    assert!(!reason.contains("not hosted"), "{reason}");
    assert!(reason.contains("no CurseForge Core API key"), "{reason}");
    assert!(reason.contains("`modpack auth`"), "{reason}");
    assert!(reason.contains("MC_CURSEFORGE_API_KEY"), "{reason}");
    assert!(reason.contains("`modpack manifest`"), "{reason}");
}
