//! The shared library of the mods (`templates/library/`), copied into each mod
//! by `mod init` and `mod update` as socle files: recorded in the registry,
//! rewritten by every update, never overwritten when edited by hand.
//!
//! How the templates map into a mod:
//! - `common/`, `fabric/` and `neoforge/` are laid out as Gradle subprojects
//!   (`src/<source set>/...`). `common/` goes to the mod's shared subproject:
//!   `common` with two loaders, the loader's subproject with one. A loader's
//!   folder goes to its subproject when the mod has that loader;
//! - the template package [`TEMPLATE_PACKAGE`] becomes `<package>.vendor`, in
//!   paths and in the text of every file;
//! - NeoForge has no `client` source set: in its subproject, the library's
//!   client code goes to `main`, as the skeleton's does;
//! - a subproject's `src/gametest` holds the library's own gametests and their
//!   test mod: they stay out of mods, and only this repository's tests copy
//!   them (`tests/library/mod.rs`);
//! - `lang/<locale>.json` holds the library's strings, keys without prefix.
//!   It is written to `.mod/vendor/lang/`, and the build merges it into the
//!   mod's language file under `<modid>.vendor.`
//!   (`build-logic/src/main/kotlin/VendorLang.kt`);
//! - a file at the root of `templates/library/` is not copied.

use std::borrow::Cow;

use include_dir::{Dir, include_dir};
use mc_core::socle::Template;

use crate::error::{Error, Result};
use crate::model::mod_toml::{Loader, ModToml, check_package};

static LIBRARY: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/library");

/// The package of the library in the templates, rewritten in each copy.
pub const TEMPLATE_PACKAGE: &str = "com.payangar.socle.vendor";

/// Where the library's language files go in a mod, read by the build.
pub const LANG_DIR: &str = ".mod/vendor/lang";

/// Every file of the library, as laid out in the mod `module`.
pub fn files(module: &ModToml) -> Result<Vec<Template>> {
    check_package(&module.package)?;
    let vendor = format!("{}.vendor", module.package);
    let shared = match module.loaders.as_slice() {
        [] => {
            return Err(Error::usage(
                "mod.toml: `loaders` is empty",
                "list the mod's loaders in mod.toml: fabric, neoforge, or both",
            ));
        }
        [loader] => loader.as_str(),
        _ => "common",
    };
    let mut out = Vec::new();
    for file in all(&LIBRARY) {
        let parts: Vec<String> = file
            .path()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let (first, rest) = parts.split_first().expect("a file has a path");
        if rest.is_empty() {
            // A file at the root (a README) documents the templates.
            continue;
        }
        if rest.starts_with(&["src".to_string(), "gametest".to_string()]) {
            continue;
        }
        let path = match first.as_str() {
            "lang" => format!("{LANG_DIR}/{}", rest.join("/")),
            "common" => in_subproject(shared, rest, &vendor),
            loader => match Loader::parse(loader) {
                Some(loader) if module.loaders.contains(&loader) => {
                    in_subproject(loader.as_str(), rest, &vendor)
                }
                Some(_) => continue,
                None => panic!("templates/library/{loader}/ is no subproject"),
            },
        };
        let contents = match std::str::from_utf8(file.contents()) {
            Ok(text) if text.contains(TEMPLATE_PACKAGE) => {
                Cow::Owned(text.replace(TEMPLATE_PACKAGE, &vendor).into_bytes())
            }
            _ => Cow::Borrowed(file.contents()),
        };
        out.push(Template { path, contents });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// The path in `subproject` of a library file at `rest` (`src/<source
/// set>/...`) under its template subproject folder.
fn in_subproject(subproject: &str, rest: &[String], vendor: &str) -> String {
    let template: Vec<&str> = TEMPLATE_PACKAGE.split('.').collect();
    let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
    let start = rest
        .windows(template.len())
        .position(|w| w == template.as_slice());
    let mut path = vec![subproject];
    for (i, part) in rest.iter().enumerate() {
        if Some(i) == start {
            path.extend(vendor.split('.'));
            path.extend(&rest[i + template.len()..]);
            break;
        }
        let neoforge_client = i == 1 && *part == "client" && subproject == "neoforge";
        path.push(if neoforge_client { "main" } else { part });
    }
    path.join("/")
}

fn all(dir: &'static Dir<'static>) -> Vec<&'static include_dir::File<'static>> {
    let mut files: Vec<_> = dir.files().collect();
    for sub in dir.dirs() {
        files.extend(all(sub));
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mod_toml::Side;

    const PKG: &str = "com/payangar/swiftblades";

    fn module(loaders: &[Loader]) -> ModToml {
        ModToml {
            id: "swift_blades".into(),
            name: "Swift Blades".into(),
            package: "com.payangar.swiftblades".into(),
            side: Side::BothRequired,
            loaders: loaders.to_vec(),
            license: "MIT".into(),
            dependencies: Vec::new(),
        }
    }

    fn paths(loaders: &[Loader]) -> Vec<String> {
        files(&module(loaders))
            .unwrap()
            .into_iter()
            .map(|t| t.path)
            .collect()
    }

    fn path(parts: &[&str], subproject: &str) -> String {
        let rest: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
        in_subproject(subproject, &rest, "com.payangar.swiftblades.vendor")
    }

    #[test]
    fn the_library_goes_under_the_vendor_package_of_the_shared_subproject() {
        let both = paths(&[Loader::Fabric, Loader::Neoforge]);
        for path in [
            format!("common/src/main/java/{PKG}/vendor/package-info.java"),
            format!("common/src/main/java/{PKG}/vendor/config/ConfigSchema.java"),
            format!("common/src/test/java/{PKG}/vendor/config/ConfigSchemaTest.java"),
            format!("{LANG_DIR}/en_us.json"),
        ] {
            assert!(both.contains(&path), "{path} missing from {both:#?}");
        }
        for loader in [Loader::Fabric, Loader::Neoforge] {
            let one = paths(&[loader]);
            let shared = format!("{loader}/src/main/java/{PKG}/vendor/config/ConfigSchema.java");
            assert!(one.contains(&shared), "{shared} missing from {one:#?}");
            assert!(one.iter().all(|p| !p.starts_with("common/")), "{one:#?}");
        }
    }

    /// Every file of the templates lands somewhere, once, for every loader set.
    #[test]
    fn no_two_library_files_share_a_path() {
        for loaders in [
            &[Loader::Fabric, Loader::Neoforge][..],
            &[Loader::Fabric],
            &[Loader::Neoforge],
        ] {
            let mut all = paths(loaders);
            let count = all.len();
            all.dedup();
            assert_eq!(all.len(), count, "{loaders:?}");
        }
    }

    #[test]
    fn the_template_package_is_rewritten_in_every_file() {
        for file in files(&module(&[Loader::Fabric, Loader::Neoforge])).unwrap() {
            let text = std::str::from_utf8(&file.contents).unwrap();
            assert!(!text.contains(TEMPLATE_PACKAGE), "{}", file.path);
            assert!(!file.path.contains("com/payangar/socle"), "{}", file.path);
            if file.path.ends_with(".java") {
                assert!(
                    text.contains("com.payangar.swiftblades.vendor"),
                    "{}",
                    file.path
                );
            }
        }
    }

    #[test]
    fn a_loader_folder_maps_to_its_subproject_and_neoforge_has_no_client_set() {
        let java = [
            "src", "client", "java", "com", "payangar", "socle", "vendor", "gui", "A.java",
        ];
        assert_eq!(
            path(&java, "fabric"),
            format!("fabric/src/client/java/{PKG}/vendor/gui/A.java")
        );
        assert_eq!(
            path(&java, "neoforge"),
            format!("neoforge/src/main/java/{PKG}/vendor/gui/A.java")
        );
        // A resource outside the package keeps its place.
        let resource = ["src", "client", "resources", "assets", "icon.json"];
        assert_eq!(
            path(&resource, "fabric"),
            "fabric/src/client/resources/assets/icon.json"
        );
    }

    /// The build reads the language files where `mod update` writes them.
    #[test]
    fn the_build_reads_the_vendor_lang_dir() {
        let build = include_str!("../../templates/socle/build-logic/src/main/kotlin/VendorLang.kt");
        assert!(build.contains(&format!("\"{LANG_DIR}\"")), "{LANG_DIR}");
    }

    #[test]
    fn the_library_gametests_stay_out_of_mods() {
        assert!(LIBRARY.get_dir("fabric/src/gametest").is_some());
        for loaders in [
            &[Loader::Fabric, Loader::Neoforge][..],
            &[Loader::Fabric],
            &[Loader::Neoforge],
        ] {
            let all = paths(loaders);
            assert!(
                all.iter().all(|p| !p.contains("/src/gametest/")),
                "{all:#?}"
            );
        }
    }

    #[test]
    fn a_mod_without_loaders_is_refused() {
        assert!(files(&module(&[])).is_err());
    }

    #[test]
    fn a_package_that_escapes_the_repository_is_refused() {
        let mut module = module(&[Loader::Fabric]);
        module.package = "com.x/../../etc".into();
        assert!(files(&module).is_err());
    }
}
