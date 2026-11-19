//! The shared crate knows no project type: no path of a project type's socle
//! directory, no type only one binary uses. Each binary passes what is its
//! own (its socle, its templates, its init values).

use std::fs;
use std::path::{Path, PathBuf};

/// Types the `modpack` crate defines for itself.
const MODPACK_TYPES: &[&str] = &[
    "Manifest",
    "Variants",
    "Resolved",
    "InitValues",
    "Installed",
    "Instances",
    "InstanceState",
    "PackBuild",
    "PackPlatform",
    "Eligibility",
    "Override",
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_shared_crate_names_no_modpack_path_nor_type() {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(!files.is_empty());
    let mut found = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).unwrap();
        for (n, line) in text.lines().enumerate() {
            let at = format!("{}:{}", file.display(), n + 1);
            if line.contains(".modpack/") {
                found.push(format!("{at}: `.modpack/`"));
            }
            for word in line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if MODPACK_TYPES.contains(&word) {
                    found.push(format!("{at}: `{word}`"));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "modpack references:\n{}",
        found.join("\n")
    );
}
