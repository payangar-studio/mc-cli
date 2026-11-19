//! The shared library draws its widgets in code: it ships no texture. Images
//! are allowed only as screenshot references of its client gametests
//! (`<subproject>/src/gametest/resources/templates/`), which never reach a
//! mod's jar.

use std::fs;
use std::path::{Path, PathBuf};

mod library;

/// Extensions of the images a game or a mod loads as textures.
const IMAGES: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "tga", "webp", "psd", "dds", "ktx",
];

/// The files under `root` that are textures: an image outside the screenshot
/// references, or anything under a `textures` folder.
fn textures(root: &Path) -> Vec<PathBuf> {
    library::walk(root)
        .into_iter()
        .filter(|file| {
            let relative = file.strip_prefix(root).unwrap();
            let parts: Vec<&str> = relative.iter().map(|p| p.to_str().unwrap()).collect();
            let reference = parts.len() == 6
                && parts[1..5] == ["src", "gametest", "resources", "templates"]
                && file.extension().is_some_and(|e| e == "png");
            let image = file
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| IMAGES.contains(&e.to_lowercase().as_str()));
            parts.contains(&"textures") || image && !reference
        })
        .collect()
}

#[test]
fn the_library_ships_no_texture() {
    let found = textures(&library::templates());
    assert!(
        found.is_empty(),
        "the shared library draws its widgets in code, yet holds textures: {found:?}"
    );
}

/// The check catches a texture, wherever it is, and lets a screenshot
/// reference through.
#[test]
fn a_texture_in_the_library_is_caught() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    for file in [
        "common/src/client/resources/assets/vendor/textures/gui/button.png",
        "common/src/client/resources/assets/vendor/gui/knob.png",
        "fabric/src/main/resources/textures/readme.txt",
        "fabric/src/gametest/resources/templates/vendor_config_screen.png",
    ] {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"").unwrap();
    }
    let mut found: Vec<String> = textures(root)
        .iter()
        .map(|p| {
            p.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            "common/src/client/resources/assets/vendor/gui/knob.png",
            "common/src/client/resources/assets/vendor/textures/gui/button.png",
            "fabric/src/main/resources/textures/readme.txt",
        ]
    );
}
