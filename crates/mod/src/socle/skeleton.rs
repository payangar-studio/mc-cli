//! The mod's own files, written once by `mod init`: `mod.toml`, the subproject
//! build scripts, the entrypoints, the page and `changelogs/`.
//!
//! Layout:
//! - one subproject per loader (`fabric/`, `neoforge/`), plus `common/` for
//!   the code they share when there are two. A single-loader mod has only its
//!   loader's subproject, which then holds the shared code too;
//! - the shared code is `<package>.CommonEntrypoint`, the client code
//!   `<package>.client.ClientEntrypoint`. Loom's `client` source set holds the
//!   client code in `common/` and `fabric/`; a NeoForge-only mod, without
//!   Loom, keeps it in `main`;
//! - each loader calls them from its entrypoints, in `<package>.<loader>`. The
//!   Fabric ones are named in the generated `fabric.mod.json`
//!   (`build-logic/src/main/kotlin/ModMetadata.kt`): renaming them breaks it;
//! - a server-only mod has no client code.

use crate::model::mod_toml::{Loader, ModToml};

/// A file of the skeleton: path relative to the repository root with `/`
/// separators, and its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub contents: String,
}

/// Every file of the skeleton of `module`.
pub fn files(module: &ModToml) -> Vec<File> {
    let fill = |template: &str| {
        template
            .replace("{{package}}", &module.package)
            .replace("{{id}}", &module.id)
            .replace("{{name}}", &module.name)
            .replace("{{name_toml}}", &toml_escape(&module.name))
            .replace("{{license_toml}}", &toml_escape(&module.license))
    };
    let package = module.package.replace('.', "/");
    let both = module.loaders.len() > 1;
    let shared = if both {
        "common"
    } else {
        module.loaders[0].as_str()
    };
    let has = |loader| module.loaders.contains(&loader);
    let client = module.side.has_client();
    // The source set of the shared client code: Loom's `client`, or `main`.
    let client_set = if shared == Loader::Neoforge.as_str() {
        "main"
    } else {
        "client"
    };

    let mut out = Vec::new();
    let mut add = |path: String, template: &str| {
        out.push(File {
            path,
            contents: fill(template),
        })
    };
    add("mod.toml".into(), &module.to_toml());
    add(
        "page/page.toml".into(),
        include_str!("../../templates/skeleton/page.toml"),
    );
    add(
        "page/description.md".into(),
        include_str!("../../templates/skeleton/description.md"),
    );
    add(
        "changelogs/README.md".into(),
        include_str!("../../templates/skeleton/changelogs-README.md"),
    );
    if both {
        add(
            "common/build.gradle.kts".into(),
            include_str!("../../templates/skeleton/common.build.gradle.kts"),
        );
    }
    add(
        format!("{shared}/src/main/java/{package}/CommonEntrypoint.java"),
        include_str!("../../templates/skeleton/CommonEntrypoint.java"),
    );
    if client {
        add(
            format!("{shared}/src/{client_set}/java/{package}/client/ClientEntrypoint.java"),
            include_str!("../../templates/skeleton/ClientEntrypoint.java"),
        );
    }
    if has(Loader::Fabric) {
        add(
            "fabric/build.gradle.kts".into(),
            include_str!("../../templates/skeleton/fabric.build.gradle.kts"),
        );
        add(
            format!("fabric/src/main/java/{package}/fabric/FabricEntrypoint.java"),
            include_str!("../../templates/skeleton/FabricEntrypoint.java"),
        );
        if client {
            add(
                format!(
                    "fabric/src/client/java/{package}/fabric/client/FabricClientEntrypoint.java"
                ),
                include_str!("../../templates/skeleton/FabricClientEntrypoint.java"),
            );
        }
    }
    if has(Loader::Neoforge) {
        add(
            "neoforge/build.gradle.kts".into(),
            include_str!("../../templates/skeleton/neoforge.build.gradle.kts"),
        );
        add(
            format!("neoforge/src/main/java/{package}/neoforge/NeoForgeEntrypoint.java"),
            include_str!("../../templates/skeleton/NeoForgeEntrypoint.java"),
        );
        if client {
            add(
                format!(
                    "neoforge/src/main/java/{package}/neoforge/client/NeoForgeClientEntrypoint.java"
                ),
                include_str!("../../templates/skeleton/NeoForgeClientEntrypoint.java"),
            );
        }
    }
    out
}

/// The content of a TOML basic string.
fn toml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mod_toml::Side;

    fn module(loaders: &[Loader], side: Side) -> ModToml {
        ModToml {
            id: "swift_blades".into(),
            name: "Swift \"Blades\"".into(),
            package: "com.payangar.swiftblades".into(),
            side,
            loaders: loaders.to_vec(),
            license: "MIT".into(),
            dependencies: vec![],
        }
    }

    fn paths(module: &ModToml) -> Vec<String> {
        let mut paths: Vec<_> = files(module).into_iter().map(|f| f.path).collect();
        paths.sort();
        paths
    }

    const PKG: &str = "com/payangar/swiftblades";

    #[test]
    fn two_loaders_share_common() {
        let paths = paths(&module(
            &[Loader::Fabric, Loader::Neoforge],
            Side::BothRequired,
        ));
        for path in [
            "common/build.gradle.kts".to_owned(),
            format!("common/src/main/java/{PKG}/CommonEntrypoint.java"),
            format!("common/src/client/java/{PKG}/client/ClientEntrypoint.java"),
            format!("fabric/src/main/java/{PKG}/fabric/FabricEntrypoint.java"),
            format!("fabric/src/client/java/{PKG}/fabric/client/FabricClientEntrypoint.java"),
            format!("neoforge/src/main/java/{PKG}/neoforge/NeoForgeEntrypoint.java"),
            format!("neoforge/src/main/java/{PKG}/neoforge/client/NeoForgeClientEntrypoint.java"),
            "mod.toml".to_owned(),
            "page/page.toml".to_owned(),
            "changelogs/README.md".to_owned(),
        ] {
            assert!(paths.contains(&path), "{path} missing from {paths:#?}");
        }
    }

    #[test]
    fn one_loader_has_only_its_subproject() {
        let fabric = paths(&module(&[Loader::Fabric], Side::Client));
        assert!(
            fabric
                .iter()
                .all(|p| !p.starts_with("common/") && !p.starts_with("neoforge/"))
        );
        assert!(fabric.contains(&format!("fabric/src/main/java/{PKG}/CommonEntrypoint.java")));
        assert!(fabric.contains(&format!(
            "fabric/src/client/java/{PKG}/client/ClientEntrypoint.java"
        )));

        let neoforge = paths(&module(&[Loader::Neoforge], Side::BothOptional));
        assert!(
            neoforge
                .iter()
                .all(|p| !p.starts_with("common/") && !p.starts_with("fabric/"))
        );
        assert!(neoforge.contains(&format!(
            "neoforge/src/main/java/{PKG}/client/ClientEntrypoint.java"
        )));
    }

    #[test]
    fn a_server_mod_has_no_client_code() {
        let paths = paths(&module(&[Loader::Fabric, Loader::Neoforge], Side::Server));
        assert!(paths.iter().all(|p| !p.contains("/client/")), "{paths:#?}");
    }

    #[test]
    fn templates_are_filled() {
        let files = files(&module(
            &[Loader::Fabric, Loader::Neoforge],
            Side::BothRequired,
        ));
        for file in &files {
            assert!(
                !file.contents.contains("{{"),
                "{} keeps a placeholder",
                file.path
            );
        }
        let common = files
            .iter()
            .find(|f| f.path.ends_with("/CommonEntrypoint.java"))
            .unwrap();
        assert!(
            common
                .contents
                .starts_with("package com.payangar.swiftblades;\n")
        );
        assert!(common.contents.contains("MOD_ID = \"swift_blades\""));
        let page = files.iter().find(|f| f.path == "page/page.toml").unwrap();
        let page: toml::Table = toml::from_str(&page.contents).unwrap();
        assert_eq!(page["name"].as_str(), Some("Swift \"Blades\""));
        assert_eq!(page["modrinth"]["license"].as_str(), Some("MIT"));
    }
}
