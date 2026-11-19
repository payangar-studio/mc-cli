//! The shared library (`templates/library/`) in the tests. `mod init` lays the
//! library out in each mod, except its gametests: they stay out of mods, and
//! [`copy_gametests`] adds them to a mod made by `mod init`, with their test
//! mod declared in the mod's gametest mod.

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

/// Copies the library's Fabric gametests (`fabric/src/gametest` of the
/// templates) into the mod `root` whose package is `package`, the template
/// package rewritten to the mod's vendor package, and declares their
/// entrypoints in the mod's gametest mod.
pub fn copy_gametests(root: &Path, package: &str) {
    let vendor = format!("{package}.vendor");
    let gametests = templates().join("fabric/src/gametest");
    let template: Vec<&str> = LIBRARY_PACKAGE.split('.').collect();
    let mut copied = 0;
    for entry in walk(&gametests) {
        let relative = entry.strip_prefix(&gametests).unwrap();
        let parts: Vec<&str> = relative.iter().map(|p| p.to_str().unwrap()).collect();
        let mut target = root.join("fabric/src/gametest");
        match parts
            .windows(template.len())
            .position(|w| w == template.as_slice())
        {
            Some(start) => {
                target.extend(&parts[..start]);
                target.extend(vendor.split('.'));
                target.extend(&parts[start + template.len()..]);
            }
            // A resource outside the package: a screenshot reference.
            None => target.extend(&parts),
        }
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let bytes = fs::read(&entry).unwrap();
        match String::from_utf8(bytes) {
            Ok(text) => fs::write(&target, text.replace(LIBRARY_PACKAGE, &vendor)).unwrap(),
            Err(binary) => fs::write(&target, binary.into_bytes()).unwrap(),
        }
        copied += 1;
    }
    assert!(copied > 0, "no library gametest in {}", gametests.display());
    declare_client_gametest(root, &vendor);
}

/// Adds the library's client gametests, and the test mod of the network ones
/// (`main` and `client` entrypoints), to the entrypoints of the Fabric test
/// mod. Edited as text: the loader wants `schemaVersion` first, and a JSON
/// round trip would sort the keys.
fn declare_client_gametest(root: &Path, vendor: &str) {
    let file = root.join("fabric/src/gametest/resources/fabric.mod.json");
    let text = fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
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
