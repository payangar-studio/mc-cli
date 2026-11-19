//! Copies the shared library (`templates/library/`) into a mod made by
//! `mod init`, as `mod init` will once it does it itself: each subproject's
//! sources and resources into the mod's matching subproject, the template
//! package rewritten to the mod's vendor package, the translation file merged
//! into the mod's language file under `<modid>.vendor.*`, and the library's
//! client gametests and their test mod declared in the mod's test mod.

// Each test file uses its part of the helpers.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// The template package of the shared library.
pub const LIBRARY_PACKAGE: &str = "com.payangar.socle.vendor";

/// The library's templates.
pub fn templates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/library")
}

/// Copies the library and its tests into the mod `root` whose id is `id` and
/// package `package`. `common/` of the templates goes to `shared` (`common`
/// with two loaders, the loader's subproject with one); `fabric/` and
/// `neoforge/` go to their subproject when the mod has it. A NeoForge-only mod
/// has no `client` source set: its client code lives in `main`. The socle's
/// build already runs JUnit in every subproject.
pub fn copy(root: &Path, id: &str, package: &str, shared: &str) {
    let templates = templates();
    let vendor = format!("{package}.vendor");
    let mut copied = 0;
    for entry in walk(&templates) {
        let relative = entry.strip_prefix(&templates).unwrap();
        let parts: Vec<&str> = relative.iter().map(|p| p.to_str().unwrap()).collect();
        if parts[0] == "lang" {
            continue;
        }
        let subproject = match parts[0] {
            "common" => shared,
            loader @ ("fabric" | "neoforge") if root.join(loader).is_dir() => loader,
            "fabric" | "neoforge" => continue,
            other => panic!("unknown library folder {other}"),
        };
        let target = root.join(target_path(&parts, subproject, &vendor));
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let bytes = fs::read(&entry).unwrap();
        match String::from_utf8(bytes) {
            Ok(text) => fs::write(&target, text.replace(LIBRARY_PACKAGE, &vendor)).unwrap(),
            Err(binary) => fs::write(&target, binary.into_bytes()).unwrap(),
        }
        copied += 1;
    }
    assert!(copied > 0, "no library file in {}", templates.display());
    merge_lang(root, id, shared);
    declare_client_gametest(root, &vendor);
}

/// Where the library file at `parts` (relative to the templates) goes in the
/// mod: the subproject replaced, the template package directories by those of
/// `vendor`, and `client` by `main` in a NeoForge-only mod.
fn target_path(parts: &[&str], subproject: &str, vendor: &str) -> PathBuf {
    let template: Vec<&str> = LIBRARY_PACKAGE.split('.').collect();
    let mut path = PathBuf::from(subproject);
    let rest = &parts[1..];
    match rest
        .windows(template.len())
        .position(|w| w == template.as_slice())
    {
        Some(start) => {
            for (i, part) in rest[..start].iter().enumerate() {
                let neoforge_client = i == 1 && *part == "client" && subproject == "neoforge";
                path.push(if neoforge_client { "main" } else { part });
            }
            path.extend(vendor.split('.'));
            path.extend(&rest[start + template.len()..]);
        }
        // A resource outside the package: a screenshot reference.
        None => path.extend(rest),
    }
    path
}

/// Merges `lang/<locale>.json` of the templates into the mod's language file,
/// each key under `<id>.vendor.`.
fn merge_lang(root: &Path, id: &str, shared: &str) {
    for entry in fs::read_dir(templates().join("lang")).unwrap() {
        let path = entry.unwrap().path();
        let library: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let target = root
            .join(shared)
            .join("src/main/resources/assets")
            .join(id)
            .join("lang")
            .join(path.file_name().unwrap());
        let mut merged: serde_json::Map<String, serde_json::Value> = fs::read_to_string(&target)
            .map(|text| serde_json::from_str(&text).unwrap())
            .unwrap_or_default();
        for (key, value) in library {
            merged.insert(format!("{id}.vendor.{key}"), value);
        }
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, serde_json::to_string_pretty(&merged).unwrap()).unwrap();
    }
}

/// Adds the library's client gametests, and the test mod of the network ones
/// (`main` and `client` entrypoints), to the entrypoints of the Fabric test
/// mod, when the mod has one. Edited as text: the loader wants
/// `schemaVersion` first, and a JSON round trip would sort the keys.
fn declare_client_gametest(root: &Path, vendor: &str) {
    let file = root.join("fabric/src/gametest/resources/fabric.mod.json");
    let Ok(text) = fs::read_to_string(&file) else {
        return;
    };
    let entrypoints = "\"entrypoints\": {";
    let list = "\"fabric-client-gametest\": [";
    for anchor in [entrypoints, list] {
        assert!(text.contains(anchor), "no `{anchor}` in {}", file.display());
    }
    let test_mod = format!(
        "{entrypoints}\"main\": [\"{vendor}.network.NetworkTestMod\"], \
         \"client\": [\"{vendor}.network.NetworkTestModClient\"], "
    );
    let gametests = format!(
        "{list}\"{vendor}.gui.ConfigScreenClientGameTest\", \
         \"{vendor}.network.ConfigSyncClientGameTest\", "
    );
    let text = text
        .replacen(entrypoints, &test_mod, 1)
        .replacen(list, &gametests, 1);
    fs::write(&file, text).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert!(json["entrypoints"]["fabric-client-gametest"].is_array());
}

/// Every file under `dir`, recursively.
pub fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}
