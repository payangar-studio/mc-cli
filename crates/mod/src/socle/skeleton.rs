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
//! - a server-only mod has no client code;
//! - every mod but a client-only one declares its config schema and the
//!   shared library's network (`ModNetwork`) in `CommonEntrypoint`, and its
//!   loaders' entrypoints register it: the side matrix (`mod test`) expects
//!   the side check of a mod required on both sides;
//! - one example test per layer the loaders run (`docs/mod-tests.md`): unit
//!   tests beside the shared code, a Fabric Loader one in `fabric/`, NeoForge
//!   integration tests in `neoforge/`, and the gametests of each loader in its
//!   `src/gametest`, a test mod `<id>_gametest` whose Fabric client gametest
//!   compares a render with `templates/example_world.png`, taken in CI.

use crate::model::mod_toml::{Loader, ModToml, Side};

/// A file of the skeleton: path relative to the repository root with `/`
/// separators, and its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub path: String,
    pub contents: Vec<u8>,
}

/// Every file of the skeleton of `module`.
pub fn files(module: &ModToml) -> Vec<File> {
    let values = [
        ("package", module.package.clone()),
        ("id", module.id.clone()),
        ("name", module.name.clone()),
        ("name_toml", toml_escape(&module.name)),
        ("license_toml", toml_escape(&module.license)),
        (
            "mod_side",
            module.side.as_str().replace('-', "_").to_uppercase(),
        ),
    ];
    let fill = |template: &str| fill(template, &values);
    let package = module.package.replace('.', "/");
    let both = module.loaders.len() > 1;
    let shared = if both {
        "common"
    } else {
        module.loaders[0].as_str()
    };
    let has = |loader| module.loaders.contains(&loader);
    let client = module.side.has_client();
    // The shared library's network: registered by every mod but a client-only
    // one, which has no server to talk to; on the client too when the mod is
    // on both sides.
    let network = module.side != Side::Client;
    let client_network = network && client;
    let pick =
        |plain: &'static str, wired: &'static str, wire: bool| if wire { wired } else { plain };
    // The source set of the shared client code: Loom's `client`, or `main`.
    let client_set = if shared == Loader::Neoforge.as_str() {
        "main"
    } else {
        "client"
    };

    // mod.toml is written as serialized, never filled: it holds the values.
    let mut out = vec![File {
        path: "mod.toml".into(),
        contents: module.to_toml().into_bytes(),
    }];
    let mut add = |path: String, template: &str| {
        out.push(File {
            path,
            contents: fill(template).into_bytes(),
        })
    };
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
        pick(
            include_str!("../../templates/skeleton/CommonEntrypoint.java"),
            include_str!("../../templates/skeleton/CommonEntrypointNetwork.java"),
            network,
        ),
    );
    add(
        format!("{shared}/src/test/java/{package}/GameBootstrap.java"),
        include_str!("../../templates/skeleton/GameBootstrap.java"),
    );
    add(
        format!("{shared}/src/test/java/{package}/ExampleUnitTest.java"),
        include_str!("../../templates/skeleton/ExampleUnitTest.java"),
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
            pick(
                include_str!("../../templates/skeleton/FabricEntrypoint.java"),
                include_str!("../../templates/skeleton/FabricEntrypointNetwork.java"),
                network,
            ),
        );
        if client {
            add(
                format!(
                    "fabric/src/client/java/{package}/fabric/client/FabricClientEntrypoint.java"
                ),
                pick(
                    include_str!("../../templates/skeleton/FabricClientEntrypoint.java"),
                    include_str!("../../templates/skeleton/FabricClientEntrypointNetwork.java"),
                    client_network,
                ),
            );
        }
        add(
            format!("fabric/src/test/java/{package}/fabric/FabricLoaderTest.java"),
            include_str!("../../templates/skeleton/FabricLoaderTest.java"),
        );
        add(
            "fabric/src/gametest/resources/fabric.mod.json".into(),
            include_str!("../../templates/skeleton/fabric.gametest.mod.json"),
        );
        add(
            format!("fabric/src/gametest/java/{package}/gametest/ExampleServerGameTest.java"),
            include_str!("../../templates/skeleton/FabricExampleServerGameTest.java"),
        );
        add(
            format!("fabric/src/gametest/java/{package}/gametest/ExampleClientGameTest.java"),
            include_str!("../../templates/skeleton/FabricExampleClientGameTest.java"),
        );
    }
    if has(Loader::Neoforge) {
        add(
            "neoforge/build.gradle.kts".into(),
            include_str!("../../templates/skeleton/neoforge.build.gradle.kts"),
        );
        add(
            format!("neoforge/src/main/java/{package}/neoforge/NeoForgeEntrypoint.java"),
            pick(
                include_str!("../../templates/skeleton/NeoForgeEntrypoint.java"),
                include_str!("../../templates/skeleton/NeoForgeEntrypointNetwork.java"),
                network,
            ),
        );
        if client {
            add(
                format!(
                    "neoforge/src/main/java/{package}/neoforge/client/NeoForgeClientEntrypoint.java"
                ),
                pick(
                    include_str!("../../templates/skeleton/NeoForgeClientEntrypoint.java"),
                    include_str!("../../templates/skeleton/NeoForgeClientEntrypointNetwork.java"),
                    client_network,
                ),
            );
        }
        add(
            format!("neoforge/src/test/java/{package}/neoforge/ExampleIntegrationTest.java"),
            include_str!("../../templates/skeleton/NeoForgeExampleIntegrationTest.java"),
        );
        add(
            "neoforge/src/gametest/resources/META-INF/neoforge.mods.toml".into(),
            include_str!("../../templates/skeleton/neoforge.gametest.mods.toml"),
        );
        add(
            format!("neoforge/src/gametest/java/{package}/gametest/ExampleServerGameTest.java"),
            include_str!("../../templates/skeleton/NeoForgeExampleServerGameTest.java"),
        );
    }
    if has(Loader::Fabric) {
        // The reference render of the example client gametest, taken in CI.
        out.push(File {
            path: "fabric/src/gametest/resources/templates/example_world.png".into(),
            contents: include_bytes!("../../templates/skeleton/example_world.png").to_vec(),
        });
    }
    out
}

/// `template` with each `{{key}}` of `values` replaced, in one pass: a value
/// that holds `{{...}}` is written as it is. Unknown placeholders are kept.
pub(crate) fn fill(template: &str, values: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let value = after.find("}}").and_then(|end| {
            let key = &after[..end];
            values
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (v, end + 2))
        });
        match value {
            Some((value, len)) => {
                out.push_str(value);
                rest = &after[len..];
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
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
            // One example test per layer, where each loader runs it.
            format!("common/src/test/java/{PKG}/GameBootstrap.java"),
            format!("common/src/test/java/{PKG}/ExampleUnitTest.java"),
            format!("fabric/src/test/java/{PKG}/fabric/FabricLoaderTest.java"),
            "fabric/src/gametest/resources/fabric.mod.json".to_owned(),
            format!("fabric/src/gametest/java/{PKG}/gametest/ExampleServerGameTest.java"),
            format!("fabric/src/gametest/java/{PKG}/gametest/ExampleClientGameTest.java"),
            "fabric/src/gametest/resources/templates/example_world.png".to_owned(),
            format!("neoforge/src/test/java/{PKG}/neoforge/ExampleIntegrationTest.java"),
            "neoforge/src/gametest/resources/META-INF/neoforge.mods.toml".to_owned(),
            format!("neoforge/src/gametest/java/{PKG}/gametest/ExampleServerGameTest.java"),
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

        assert!(fabric.contains(&format!("fabric/src/test/java/{PKG}/ExampleUnitTest.java")));

        let neoforge = paths(&module(&[Loader::Neoforge], Side::BothOptional));
        assert!(neoforge.contains(&format!(
            "neoforge/src/test/java/{PKG}/ExampleUnitTest.java"
        )));
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
        for file in files.iter().filter(|f| !f.path.ends_with(".png")) {
            let text = std::str::from_utf8(&file.contents).unwrap();
            assert!(!text.contains("{{"), "{} keeps a placeholder", file.path);
        }
        let text = |suffix: &str| {
            let file = files.iter().find(|f| f.path.ends_with(suffix)).unwrap();
            std::str::from_utf8(&file.contents).unwrap()
        };
        let common = text("/CommonEntrypoint.java");
        assert!(common.starts_with("package com.payangar.swiftblades;\n"));
        assert!(common.contains("MOD_ID = \"swift_blades\""));
        let page: toml::Table = toml::from_str(text("page/page.toml")).unwrap();
        assert_eq!(page["name"].as_str(), Some("Swift \"Blades\""));
        assert_eq!(page["modrinth"]["license"].as_str(), Some("MIT"));
    }

    #[test]
    fn the_network_is_registered_by_every_side_that_has_one() {
        let text = |side: Side, suffix: &str| {
            let files = files(&module(&[Loader::Fabric, Loader::Neoforge], side));
            files
                .into_iter()
                .find(|f| f.path.ends_with(suffix))
                .map(|f| String::from_utf8(f.contents).unwrap())
        };
        let common = text(Side::BothOptional, "/CommonEntrypoint.java").unwrap();
        assert!(common.contains("ModSide.BOTH_OPTIONAL"), "{common}");
        assert!(common.contains("import com.payangar.swiftblades.vendor.network.ModNetwork;"));
        for side in [Side::Server, Side::BothRequired, Side::BothOptional] {
            for (file, call) in [
                ("/FabricEntrypoint.java", "FabricNetwork.register("),
                ("/NeoForgeEntrypoint.java", "NeoForgeNetwork.register("),
            ] {
                assert!(text(side, file).unwrap().contains(call), "{side} {file}");
            }
        }
        for side in [Side::BothRequired, Side::BothOptional] {
            for (file, call) in [
                (
                    "/FabricClientEntrypoint.java",
                    "FabricNetworkClient.register(",
                ),
                (
                    "/NeoForgeClientEntrypoint.java",
                    "NeoForgeNetworkClient.register(",
                ),
            ] {
                assert!(text(side, file).unwrap().contains(call), "{side} {file}");
            }
        }
        // A client-only mod has no network.
        for file in [
            "/CommonEntrypoint.java",
            "/FabricEntrypoint.java",
            "/FabricClientEntrypoint.java",
            "/NeoForgeEntrypoint.java",
            "/NeoForgeClientEntrypoint.java",
        ] {
            assert!(
                !text(Side::Client, file).unwrap().contains("Network"),
                "{file}"
            );
        }
    }

    #[test]
    fn values_are_never_read_as_placeholders() {
        let mut module = module(&[Loader::Fabric], Side::Client);
        module.name = "Tools {{package}} {{id}}".into();
        module.license = "{{name}}".into();
        let files = files(&module);
        let file = |path: &str| {
            std::str::from_utf8(&files.iter().find(|f| f.path == path).unwrap().contents).unwrap()
        };
        let toml: ModToml = toml::from_str(file("mod.toml")).unwrap();
        assert_eq!(toml.name, module.name);
        assert_eq!(toml.license, module.license);
        assert_eq!(file("page/description.md"), "# Tools {{package}} {{id}}\n");
        let page: toml::Table = toml::from_str(file("page/page.toml")).unwrap();
        assert_eq!(page["modrinth"]["license"].as_str(), Some("{{name}}"));
    }
}
