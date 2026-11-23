//! Builds of projects made by `mod init`, with their own Gradle wrapper. They
//! download Gradle, the loaders and Minecraft, so they are `#[ignore]`: CI runs
//! them in their own job (`cargo test -p mc-mod --test gradle_build --
//! --ignored`). They need a JDK 25 (`JAVA_HOME` or on the path).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};

use assert_cmd::Command;

mod library;

/// The package of the mods `init` makes.
const PACKAGE: &str = "com.payangar.swiftblades";

/// `PACKAGE` as a `/`-separated path.
fn package_dir() -> String {
    PACKAGE.replace('.', "/")
}

/// A project made by `mod init` in `<tmp>/mod`.
fn init(loaders: &str, side: &str) -> (tempfile::TempDir, PathBuf) {
    init_named(loaders, side, "Swift Blades")
}

/// `init`, with the mod named `name`.
fn init_named(loaders: &str, side: &str, name: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = init_in(tmp.path(), "swift_blades", PACKAGE, name, loaders, side);
    (tmp, root)
}

/// A project made by `mod init` in `<dir>/<id>`.
fn init_in(dir: &Path, id: &str, package: &str, name: &str, loaders: &str, side: &str) -> PathBuf {
    Command::cargo_bin("mod")
        .unwrap()
        .current_dir(dir)
        .env("XDG_CONFIG_HOME", dir.join(".machine-config"))
        .args(["init", id, "--license=MIT", "--mc-version=26.3"])
        .arg(format!("--id={id}"))
        .arg(format!("--package={package}"))
        .arg(format!("--name={name}"))
        .arg(format!("--loaders={loaders}"))
        .arg(format!("--side={side}"))
        .assert()
        .success();
    dir.join(id)
}

fn gradle(root: &Path, args: &[&str]) -> Output {
    let output = Process::new(root.join("gradlew"))
        .args(["--console=plain", "--stacktrace"])
        .args(args)
        // A test never publishes, whatever the machine's environment holds.
        .env_remove("MC_MODRINTH_TOKEN")
        .env_remove("MC_CURSEFORGE_UPLOAD_TOKEN")
        .current_dir(root)
        .output()
        .unwrap();
    eprintln!(
        "./gradlew {}\n{}{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn gradle_ok(root: &Path, args: &[&str]) {
    let output = gradle(root, args);
    assert!(
        output.status.success(),
        "./gradlew {} failed",
        args.join(" ")
    );
}

fn jar(root: &Path, loader: &str) -> PathBuf {
    root.join(format!(
        "{loader}/build/libs/swift_blades-{loader}-0.0.0.jar"
    ))
}

fn fabric_mod_json(root: &Path) -> serde_json::Value {
    let text =
        fs::read_to_string(root.join("fabric/build/resources/main/fabric.mod.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn neoforge_mods_toml(root: &Path) -> toml::Table {
    let text =
        fs::read_to_string(root.join("neoforge/build/resources/main/META-INF/neoforge.mods.toml"))
            .unwrap();
    toml::from_str(&text).unwrap()
}

#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft"]
fn builds_with_both_loaders() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    gradle_ok(&root, &["build"]);
    assert!(jar(&root, "fabric").is_file());
    assert!(jar(&root, "neoforge").is_file());
    assert_library_tests_passed(&root, "common", COMMON_LIBRARY_TESTS);
}

#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft"]
fn builds_with_fabric_only() {
    let (_tmp, root) = init("fabric", "client");
    assert!(!root.join("common").exists());
    gradle_ok(&root, &["build"]);
    assert!(jar(&root, "fabric").is_file());
    assert_library_tests_passed(&root, "fabric", COMMON_LIBRARY_TESTS);
}

/// Configuring the build of a mod with a client side but without the
/// library's Mod Menu entrypoint resolves no Mod Menu: only the dev client of
/// `mod run` needs it then, so the other tasks do without its repository.
#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft"]
fn configuring_a_client_mod_without_the_entrypoint_resolves_no_mod_menu() {
    let (_tmp, root) = init("fabric", "both-required");
    fs::remove_file(root.join(format!(
        "fabric/src/client/java/{}/vendor/fabric/ModMenuEntrypoint.java",
        package_dir()
    )))
    .unwrap();
    let socle = root.join(".mod/socle.toml");
    let text = fs::read_to_string(&socle).unwrap();
    let table: toml::Table = text.parse().unwrap();
    let pinned = table["init"]["modmenu"].as_str().unwrap();
    let missing = text.replace(
        &format!("modmenu = \"{pinned}\""),
        "modmenu = \"0.0.0-missing\"",
    );
    assert_ne!(missing, text, "no Mod Menu pin to replace");
    fs::write(&socle, missing).unwrap();
    gradle_ok(&root, &["help"]);
}

/// A server mod loads in the Fabric Loader of its unit tests, which starts as
/// a client unless told otherwise.
#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft"]
fn builds_a_fabric_server_mod() {
    let (_tmp, root) = init("fabric", "server");
    gradle_ok(&root, &["build"]);
    assert!(jar(&root, "fabric").is_file());
}

/// A NeoForge-only mod has no client source set: the library's client code
/// goes into `main`, next to the NeoForge config screen extension.
#[test]
#[ignore = "downloads Gradle, NeoForge and Minecraft"]
fn builds_with_neoforge_only() {
    let (_tmp, root) = init("neoforge", "server");
    assert!(!root.join("common").exists());
    assert!(
        root.join(format!(
            "neoforge/src/main/java/{}/vendor/gui/screen/ConfigScreen.java",
            package_dir()
        ))
        .is_file()
    );
    gradle_ok(&root, &["build"]);
    assert!(jar(&root, "neoforge").is_file());
    assert_library_tests_passed(&root, "neoforge", COMMON_LIBRARY_TESTS);
}

/// A NeoForge client mod has no network: its client entrypoint registers the
/// config screen alone.
#[test]
#[ignore = "downloads Gradle, NeoForge and Minecraft"]
fn builds_a_neoforge_client_mod() {
    let (_tmp, root) = init("neoforge", "client");
    gradle_ok(&root, &[":neoforge:compileJava"]);
}

/// The NeoForge `runClient` and `runServer` of `mod run` load the mod alone:
/// ModDevGradle loads every registered mod by default, the gametest mod
/// `<id>_gametest` included. Read in the `fml.modFolders` argument of each run,
/// printed by an init script.
#[test]
#[ignore = "downloads Gradle, NeoForge and Minecraft"]
fn the_neoforge_runs_load_the_mod_without_its_test_mod() {
    let (tmp, root) = init("neoforge", "both-required");
    let script = tmp.path().join("mod-folders.init.gradle");
    fs::write(
        &script,
        r#"gradle.projectsEvaluated {
    def neoforge = rootProject.findProject(':neoforge')
    if (neoforge == null) return
    ['runClient', 'runServer'].each { name ->
        def folders = neoforge.tasks.getByName(name).allJvmArgs.find { it.startsWith('-Dfml.modFolders=') }
        println("$name $folders")
    }
}
"#,
    )
    .unwrap();
    let output = gradle(&root, &["-I", script.to_str().unwrap(), "help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for run in ["runClient", "runServer"] {
        let line = stdout
            .lines()
            .find(|l| l.starts_with(&format!("{run} ")))
            .unwrap_or_else(|| panic!("no mod folders printed for {run}"));
        assert!(line.contains("swift_blades%%"), "{line}");
        assert!(!line.contains("swift_blades_gametest"), "{line}");
    }
}

/// The loader metadata follows `mod.toml`: the side, and each dependency for
/// the loaders it names.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft"]
fn metadata_follows_mod_toml_for_each_side() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    let original = fs::read_to_string(root.join("mod.toml")).unwrap();
    let dependencies = r#"
[[dependencies]]
id = "sodium"
type = "optional"
loaders = ["fabric"]
modrinth = "sodium"

[[dependencies]]
id = "jei"
type = "required"
loaders = ["neoforge"]

[[dependencies]]
id = "oldmod"
type = "incompatible"
"#;
    for (side, environment, neoforge_side) in [
        ("client", "client", "CLIENT"),
        ("server", "server", "SERVER"),
        ("both-required", "*", "BOTH"),
        ("both-optional", "*", "BOTH"),
    ] {
        let text = original.replace("side = \"both-required\"", &format!("side = \"{side}\""));
        fs::write(root.join("mod.toml"), format!("{text}{dependencies}")).unwrap();
        gradle_ok(
            &root,
            &[":fabric:processResources", ":neoforge:processResources"],
        );

        let fabric = fabric_mod_json(&root);
        assert_eq!(fabric["id"], "swift_blades");
        assert_eq!(fabric["environment"], environment, "{side}");
        assert_eq!(fabric["depends"]["fabric-api"], "*");
        assert_eq!(fabric["depends"]["minecraft"], "~26.3");
        assert!(
            fabric["depends"].get("jei").is_none(),
            "jei is NeoForge only"
        );
        assert_eq!(fabric["suggests"]["sodium"], "*");
        assert_eq!(fabric["breaks"]["oldmod"], "*");
        let entrypoints = fabric["entrypoints"].as_object().unwrap();
        assert_eq!(
            entrypoints.contains_key("client"),
            side != "server",
            "{side}"
        );
        assert_eq!(
            entrypoints.contains_key("modmenu"),
            side != "server",
            "{side}: Mod Menu's entrypoint follows the library's client code"
        );
        assert!(fabric.get("mixins").is_none(), "no mixin configuration");

        let neoforge = neoforge_mods_toml(&root);
        assert_eq!(neoforge["mods"][0]["modId"].as_str(), Some("swift_blades"));
        assert_eq!(neoforge["license"].as_str(), Some("MIT"));
        assert!(neoforge.get("mixins").is_none(), "no mixin configuration");
        let deps = neoforge["dependencies"]["swift_blades"].as_array().unwrap();
        let dep = |id: &str| {
            deps.iter()
                .find(|d| d["modId"].as_str() == Some(id))
                .unwrap_or_else(|| panic!("{id} missing from {deps:?}"))
        };
        for dependency in deps {
            assert_eq!(dependency["side"].as_str(), Some(neoforge_side), "{side}");
        }
        assert_eq!(dep("neoforge")["type"].as_str(), Some("required"));
        assert_eq!(
            dep("minecraft")["versionRange"].as_str(),
            Some("[26.3,26.4)")
        );
        assert_eq!(dep("jei")["type"].as_str(), Some("required"));
        assert_eq!(dep("oldmod")["type"].as_str(), Some("incompatible"));
        assert!(deps.iter().all(|d| d["modId"].as_str() != Some("sodium")));
        assert!(
            deps.iter()
                .all(|d| d["modId"].as_str() != Some("fabric-api"))
        );
    }
}

/// The client split of `common`: a client-only Minecraft class does not
/// compile from `main`, and does from `client`.
#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft"]
fn a_client_class_compiles_only_in_the_client_source_set() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    let fixture = "package com.payangar.swiftblades;\n\n\
        public final class SplitFixture {\n    \
        public static Object client() {\n        \
        return net.minecraft.client.Minecraft.getInstance();\n    }\n}\n";
    let main = root.join(format!(
        "common/src/main/java/{}/SplitFixture.java",
        package_dir()
    ));
    fs::write(&main, fixture).unwrap();
    let output = gradle(&root, &[":common:compileJava"]);
    assert!(
        !output.status.success(),
        "a client class compiled in common's main"
    );
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        log.contains("package net.minecraft.client does not exist"),
        "{log}"
    );

    fs::remove_file(&main).unwrap();
    let client = root.join(format!(
        "common/src/client/java/{}/SplitFixture.java",
        package_dir()
    ));
    fs::write(&client, fixture).unwrap();
    gradle_ok(&root, &[":common:compileClientJava"]);
}

/// Every control character of the name is escaped in neoforge.mods.toml,
/// DEL included, which TOML forbids in a basic string.
#[test]
#[ignore = "downloads Gradle and NeoForge"]
fn control_characters_of_the_name_are_escaped() {
    let name = "Swift\u{7f}Blades\u{1}";
    let (_tmp, root) = init_named("neoforge", "both-required", name);
    gradle_ok(&root, &[":neoforge:generateModMetadata"]);
    let text = fs::read_to_string(
        root.join("neoforge/build/generated/modMetadata/META-INF/neoforge.mods.toml"),
    )
    .unwrap();
    let toml: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(toml["mods"][0]["displayName"].as_str(), Some(name));
}

/// A `[[dependencies]]` written as something else than a table is reported
/// with its place in mod.toml, not with a parser's stack trace.
#[test]
#[ignore = "downloads Gradle and the Gradle plugins"]
fn a_malformed_dependency_is_named() {
    let (_tmp, root) = init("neoforge", "both-required");
    let text = fs::read_to_string(root.join("mod.toml")).unwrap();
    fs::write(
        root.join("mod.toml"),
        format!("{text}dependencies = [\"sodium\"]\n"),
    )
    .unwrap();
    let output = gradle(&root, &["help"]);
    assert!(!output.status.success());
    let log = String::from_utf8_lossy(&output.stderr);
    assert!(
        log.contains("mod.toml, dependencies[0]: must be a table"),
        "{log}"
    );
}

/// Writes, as `<PACKAGE>.<class>` in the Java sources of `dir`, a mixin whose
/// MixinExtras injector makes `StringUtil.stripColor` turn `probe` into
/// `applied`.
fn write_probe_mixin(dir: &Path, class: &str, probe: &str, applied: &str) {
    let (package, name) = class.rsplit_once('.').unwrap();
    let path = dir.join(format!(
        "{}/{}.java",
        package_dir(),
        class.replace('.', "/")
    ));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let source = format!(
        "package {PACKAGE}.{package};\n\n\
         import com.llamalad7.mixinextras.injector.ModifyReturnValue;\n\
         import net.minecraft.util.StringUtil;\n\
         import org.spongepowered.asm.mixin.Mixin;\n\
         import org.spongepowered.asm.mixin.injection.At;\n\n\
         @Mixin(StringUtil.class)\n\
         abstract class {name} {{\n    \
         @ModifyReturnValue(method = \"stripColor\", at = @At(\"RETURN\"))\n    \
         private static String probe(String original) {{\n        \
         return \"{probe}\".equals(original) ? \"{applied}\" : original;\n    }}\n}}\n"
    );
    fs::write(path, source).unwrap();
}

/// Writes the mixin configuration `path` of the mixins of `<PACKAGE>.<package>`:
/// `mixins` on both sides, `client` on the client only.
fn write_mixin_config(path: &Path, package: &str, mixins: &[&str], client: &[&str]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let config = serde_json::json!({
        "required": true,
        "package": format!("{PACKAGE}.{package}"),
        "compatibilityLevel": "JAVA_25",
        "mixins": mixins,
        "client": client,
        "injectors": { "defaultRequire": 1 },
    });
    fs::write(path, config.to_string()).unwrap();
}

/// Mixin configurations named as in the loaders' templates are declared in
/// the loader metadata and their mixins applied: `common`'s by both loaders, a
/// loader's own by that loader alone, Fabric's client one on the client.
/// Each loader's unit test, with the game loaded, reads what the MixinExtras
/// injectors did. NeoForge has no client-only configuration: one in `common`
/// fails its build with what to do instead, as does a configuration in the
/// wrong source set, which would ship undeclared.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft"]
fn mixin_configurations_are_declared_and_applied() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    let id = "swift_blades";
    let resources = |subproject: &str, source_set: &str| {
        root.join(format!("{subproject}/src/{source_set}/resources"))
    };
    let java = |subproject: &str, source_set: &str| {
        root.join(format!("{subproject}/src/{source_set}/java"))
    };

    write_mixin_config(
        &resources("common", "main").join(format!("{id}.mixins.json")),
        "mixin",
        &["CommonMixin"],
        &["client.CommonClientMixin"],
    );
    write_probe_mixin(
        &java("common", "main"),
        "mixin.CommonMixin",
        "probe-common",
        "common",
    );
    write_probe_mixin(
        &java("common", "client"),
        "mixin.client.CommonClientMixin",
        "probe-common-client",
        "common-client",
    );
    for loader in ["fabric", "neoforge"] {
        write_mixin_config(
            &resources(loader, "main").join(format!("{id}.{loader}.mixins.json")),
            &format!("{loader}.mixin"),
            &["LoaderMixin"],
            &[],
        );
        write_probe_mixin(
            &java(loader, "main"),
            &format!("{loader}.mixin.LoaderMixin"),
            &format!("probe-{loader}"),
            loader,
        );
    }
    write_mixin_config(
        &resources("fabric", "client").join(format!("{id}.client.mixins.json")),
        "fabric.client.mixin",
        &[],
        &["ClientMixin"],
    );
    write_probe_mixin(
        &java("fabric", "client"),
        "fabric.client.mixin.ClientMixin",
        "probe-fabric-client",
        "fabric-client",
    );

    // What each loader's tests read: the probe and what it becomes. Fabric's
    // run as a client, NeoForge's as a dedicated server, where no client
    // mixin applies.
    let probes = |loader: &str| -> Vec<(&'static str, &'static str)> {
        let mut probes = vec![("probe-common", "common")];
        if loader == "fabric" {
            probes.extend([
                ("probe-common-client", "common-client"),
                ("probe-fabric", "fabric"),
                ("probe-fabric-client", "fabric-client"),
                ("probe-neoforge", "probe-neoforge"),
            ]);
        } else {
            probes.extend([
                ("probe-common-client", "probe-common-client"),
                ("probe-neoforge", "neoforge"),
                ("probe-fabric", "probe-fabric"),
            ]);
        }
        probes
    };
    for loader in ["fabric", "neoforge"] {
        let checks: String = probes(loader)
            .iter()
            .map(|(probe, expected)| {
                format!(
                    "        assertEquals(\"{expected}\", StringUtil.stripColor(\"{probe}\"));\n"
                )
            })
            .collect();
        let path = java(loader, "test").join(format!("{}/MixinProbeTest.java", package_dir()));
        fs::write(
            path,
            format!(
                "package {PACKAGE};\n\n\
                 import static org.junit.jupiter.api.Assertions.assertEquals;\n\n\
                 import net.minecraft.util.StringUtil;\n\
                 import org.junit.jupiter.api.Test;\n\n\
                 class MixinProbeTest {{\n    \
                 @Test\n    \
                 void theMixinsAreApplied() {{\n{checks}    }}\n}}\n"
            ),
        )
        .unwrap();
    }

    gradle_ok(&root, &[":fabric:test", ":neoforge:test"]);
    for loader in ["fabric", "neoforge"] {
        let report = root.join(format!(
            "{loader}/build/test-results/test/TEST-{PACKAGE}.MixinProbeTest.xml"
        ));
        assert!(report.is_file(), "{loader}: the probe test did not run");
    }
    assert_eq!(
        fabric_mod_json(&root)["mixins"],
        serde_json::json!([
            format!("{id}.mixins.json"),
            format!("{id}.fabric.mixins.json"),
            { "config": format!("{id}.client.mixins.json"), "environment": "client" },
        ])
    );
    let neoforge = neoforge_mods_toml(&root);
    let configs: Vec<&str> = neoforge["mixins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["config"].as_str().unwrap())
        .collect();
    assert_eq!(
        configs,
        [
            format!("{id}.mixins.json"),
            format!("{id}.neoforge.mixins.json")
        ]
    );

    write_mixin_config(
        &resources("common", "client").join(format!("{id}.client.mixins.json")),
        "mixin.client",
        &[],
        &[],
    );
    let output = gradle(&root, &[":neoforge:generateModMetadata"]);
    assert!(!output.status.success());
    let log = String::from_utf8_lossy(&output.stderr);
    assert!(
        log.contains(&format!(
            "common/src/client/resources/{id}.client.mixins.json: NeoForge has no client-only mixin configuration"
        )),
        "{log}"
    );
    fs::remove_file(resources("common", "client").join(format!("{id}.client.mixins.json")))
        .unwrap();

    // A configuration in the wrong source set would ship undeclared.
    fs::rename(
        resources("fabric", "main").join(format!("{id}.fabric.mixins.json")),
        resources("fabric", "client").join(format!("{id}.fabric.mixins.json")),
    )
    .unwrap();
    let output = gradle(&root, &[":fabric:generateModMetadata"]);
    assert!(!output.status.success());
    let log = String::from_utf8_lossy(&output.stderr);
    assert!(
        log.contains(&format!(
            "fabric/src/client/resources/{id}.fabric.mixins.json: the build declares {id}.fabric.mixins.json from src/main/resources only"
        )),
        "{log}"
    );
}

/// Each library test class of `classes` (`<package>.<class>` under the
/// vendor package) ran in `subproject` and passed: a green build with no test
/// run proves nothing.
fn assert_library_tests_passed(root: &Path, subproject: &str, classes: &[&str]) {
    let results = root.join(subproject).join("build/test-results/test");
    for class in classes {
        let report = results.join(format!("TEST-{PACKAGE}.vendor.{class}.xml"));
        let text =
            fs::read_to_string(&report).unwrap_or_else(|e| panic!("{}: {e}", report.display()));
        assert!(!text.contains("tests=\"0\""), "{class} ran no test");
        assert!(text.contains("failures=\"0\""), "{text}");
        assert!(text.contains("errors=\"0\""), "{text}");
    }
}

/// Lines the library's client gametests log once a change of server settings
/// is applied, each made by one of them only: on the dedicated server
/// (`ConfigSyncClientGameTest`), and in single player from the config screen
/// (`ServerSettingsClientGameTest`).
const LIBRARY_GAMETEST_MARKS: &[&str] = &[
    "Server settings of vendor_network_test changed: {\"range\":16}",
    "Server settings of vendor_network_test changed: {\"range\":64}",
];

/// The library's client gametests ran in `:fabric:runClientGameTest`, up to
/// their change of server settings: the socle's own client gametest passes
/// that run as well, so a green build does not show the library's ran.
fn assert_library_gametest_ran(root: &Path) {
    let log = root.join("fabric/build/run/clientGameTest/logs/latest.log");
    let text = fs::read_to_string(&log).unwrap_or_else(|e| panic!("{}: {e}", log.display()));
    for mark in LIBRARY_GAMETEST_MARKS {
        assert!(
            text.contains(mark),
            "a library client gametest did not run: `{mark}` missing from {}",
            log.display()
        );
    }
}

/// The library tests that need no loader, in `common/` of the templates.
const COMMON_LIBRARY_TESTS: &[&str] = &[
    "config.ConfigSchemaTest",
    "config.ConfigFileTest",
    "config.ConfigValuesTest",
    "config.SettingTypeTest",
    "network.ConfigPayloadsTest",
    "network.ReceivedConfigTest",
    "network.SideCheckTest",
];

/// The shared library `mod init` lays out compiles for both loaders, and its
/// tests pass there: unit tests, the server side of the sync on NeoForge
/// with the game loaded, and, copied by the test since no mod ships them, its
/// Fabric client gametests on a dedicated server. The client gametests need a
/// display: Xvfb, as in CI.
#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and JUnit, and starts the game"]
fn library_tests_pass_in_a_generated_mod() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    library::copy_gametests(&root, PACKAGE);
    gradle_ok(&root, &["build", ":fabric:runClientGameTest"]);
    assert!(jar(&root, "fabric").is_file());
    assert!(jar(&root, "neoforge").is_file());
    assert_library_tests_passed(&root, "common", COMMON_LIBRARY_TESTS);
    assert_library_tests_passed(&root, "neoforge", &["network.ServerConfigTest"]);
    assert_library_gametest_ran(&root);
    // Mod Menu's entrypoint is declared once the library's class is there.
    assert_eq!(
        fabric_mod_json(&root)["entrypoints"]["modmenu"][0],
        format!("{PACKAGE}.vendor.fabric.ModMenuEntrypoint")
    );
}

fn read_json(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

/// Each loader's resources hold the mod's language file with the library's
/// strings merged in under `<modid>.vendor.`; a key the mod defines itself
/// wins, and a locale the library lacks is left as the mod wrote it.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft"]
fn the_language_file_merges_the_vendor_keys() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    let lang = root.join("common/src/main/resources/assets/swift_blades/lang");
    fs::create_dir_all(&lang).unwrap();
    fs::write(
        lang.join("en_us.json"),
        r#"{"item.swift_blades.blade": "Swift Blade", "swift_blades.vendor.config.scope.server": "Rules"}"#,
    )
    .unwrap();
    fs::write(
        lang.join("fr_fr.json"),
        r#"{"item.swift_blades.blade": "Lame"}"#,
    )
    .unwrap();
    gradle_ok(
        &root,
        &[":fabric:processResources", ":neoforge:processResources"],
    );
    for loader in ["fabric", "neoforge"] {
        let built = root.join(format!(
            "{loader}/build/resources/main/assets/swift_blades/lang"
        ));
        let en = read_json(&built.join("en_us.json"));
        assert_eq!(en["item.swift_blades.blade"], "Swift Blade", "{loader}");
        assert_eq!(
            en["swift_blades.vendor.config.scope.client"], "Client",
            "{loader}"
        );
        assert_eq!(
            en["swift_blades.vendor.config.scope.server"], "Rules",
            "{loader}"
        );
        assert!(en.keys().all(|k| !k.starts_with("config.")), "{en:?}");
        let fr = read_json(&built.join("fr_fr.json"));
        assert_eq!(fr.len(), 1, "{loader}: {fr:?}");
    }
    gradle_ok(&root, &[":fabric:jar", ":neoforge:jar"]);
    assert_every_language_file_merged(&root);
}

/// A language file in common's client resources, where client assets often
/// live: each jar holds it once, merged, and so does every resource folder
/// a dev run loads the mod from.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft"]
fn a_client_language_file_merges_the_vendor_keys() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    let lang = root.join("common/src/client/resources/assets/swift_blades/lang");
    fs::create_dir_all(&lang).unwrap();
    fs::write(
        lang.join("en_us.json"),
        r#"{"item.swift_blades.blade": "Swift Blade"}"#,
    )
    .unwrap();
    gradle_ok(&root, &[":fabric:jar", ":neoforge:jar"]);
    assert_every_language_file_merged(&root);
}

/// The mod's English file, in each jar and in every resource folder of the
/// mod's subprojects, holds the vendor keys: a dev run loads the mod from all
/// of these folders, and the first one holding the file wins.
fn assert_every_language_file_merged(root: &Path) {
    const EN: &str = "assets/swift_blades/lang/en_us.json";
    for loader in ["fabric", "neoforge"] {
        let path = jar(root, loader);
        let mut archive = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        let copies = archive
            .file_names()
            .filter(|n| n.as_ref().unwrap() == EN)
            .count();
        assert_eq!(copies, 1, "{}: {EN}", path.display());
        let mut text = String::new();
        std::io::Read::read_to_string(&mut archive.by_name(EN).unwrap(), &mut text).unwrap();
        let en: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(en["item.swift_blades.blade"], "Swift Blade", "{loader}");
        assert_eq!(
            en["swift_blades.vendor.config.scope.client"], "Client",
            "{loader}"
        );
    }
    let mut found = 0;
    for subproject in ["common", "fabric", "neoforge"] {
        for set in ["main", "client"] {
            let file = root.join(format!("{subproject}/build/resources/{set}/{EN}"));
            if !file.is_file() {
                continue;
            }
            found += 1;
            let en = read_json(&file);
            assert_eq!(
                en["swift_blades.vendor.config.scope.client"],
                "Client",
                "{}",
                file.display()
            );
        }
    }
    assert!(found > 0, "no resource folder holds {EN}");
}

/// Replaces the mod's `CommonEntrypoint` with one that builds a config schema
/// through its copy of the library and prints the class it got. It keeps the
/// `NETWORK` the loader's entrypoints register.
fn use_the_library(root: &Path, id: &str, package: &str) {
    let path = root.join(format!(
        "fabric/src/main/java/{}/CommonEntrypoint.java",
        package.replace('.', "/")
    ));
    fs::write(
        &path,
        format!(
            r#"package {package};

import {package}.vendor.config.ConfigSchema;
import {package}.vendor.config.ModSide;
import {package}.vendor.config.Scope;
import {package}.vendor.config.Setting;
import {package}.vendor.network.ModNetwork;

public final class CommonEntrypoint {{
    public static final String MOD_ID = "{id}";

    public static final ConfigSchema SCHEMA = schema();

    public static final ModNetwork NETWORK = new ModNetwork(SCHEMA);

    private CommonEntrypoint() {{}}

    private static ConfigSchema schema() {{
        ConfigSchema.Builder config = ConfigSchema.builder(MOD_ID, ModSide.BOTH_REQUIRED);
        config.define(Setting.integer("range", Scope.SERVER, 8, 1, 64));
        return config.build();
    }}

    public static void init() {{
        System.out.println("library of " + SCHEMA.modId() + ": " + ConfigSchema.class.getName());
    }}
}}
"#
        ),
    )
    .unwrap();
}

/// The entries of a jar, directories left out.
fn jar_entries(path: &Path) -> std::collections::BTreeSet<String> {
    let archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    archive
        .file_names()
        .map(|name| name.unwrap().into_owned())
        .filter(|name| !name.ends_with('/'))
        .collect()
}

/// Two mods carrying their own copy of the library share no class and no
/// resource but their metadata, and load together in one Fabric run: the
/// server gametests of one, with the other's jar added.
#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft"]
fn two_mods_with_the_library_load_together() {
    let tmp = tempfile::tempdir().unwrap();
    let mods = [
        ("swift_blades", PACKAGE),
        ("frost_arrows", "com.payangar.frostarrows"),
    ];
    let roots: Vec<PathBuf> = mods
        .iter()
        .map(|(id, package)| {
            let root = init_in(tmp.path(), id, package, id, "fabric", "both-required");
            use_the_library(&root, id, package);
            root
        })
        .collect();
    let other = &roots[1];
    gradle_ok(other, &[":fabric:jar"]);
    let other_jar = other.join("fabric/build/libs/frost_arrows-fabric-0.0.0.jar");

    let build = roots[0].join("fabric/build.gradle.kts");
    let mut script = fs::read_to_string(&build).unwrap();
    script.push_str(&format!(
        r#"
extensions.configure<net.fabricmc.loom.api.LoomGradleExtensionAPI> {{
    runs.named("gameTest") {{ property("fabric.addMods", "{}") }}
}}
"#,
        other_jar.display()
    ));
    fs::write(&build, script).unwrap();
    let output = gradle(&roots[0], &[":fabric:jar", ":fabric:runGameTest"]);
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "the run with both mods failed");
    for (id, package) in mods {
        let line = format!("library of {id}: {package}.vendor.config.ConfigSchema");
        assert!(log.contains(&line), "{line} missing from the run's log");
    }

    let shared: Vec<String> = jar_entries(&jar(&roots[0], "fabric"))
        .intersection(&jar_entries(&other_jar))
        .cloned()
        .collect();
    assert_eq!(shared, ["META-INF/MANIFEST.MF", "fabric.mod.json"]);
}

/// What a dry run of `publishMods` would send to `platform` for `loader`.
fn dry_run(root: &Path, loader: &str, platform: &str) -> Option<serde_json::Value> {
    let path = root.join(format!(
        "{loader}/build/publishMods/{platform}.dry-run.json"
    ));
    let text = fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

/// The dependencies of a dry run, as `slug:TYPE`.
fn dependencies(description: &serde_json::Value) -> Vec<String> {
    description["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            format!(
                "{}:{}",
                d["slug"].as_str().unwrap(),
                d["type"].as_str().unwrap()
            )
        })
        .collect()
}

fn write_page(root: &Path, modrinth: &str, curseforge: &str) {
    fs::write(
        root.join("page/page.toml"),
        format!(
            "name = \"Swift Blades\"\n\n[modrinth]\nproject_id = \"{modrinth}\"\n\n[curseforge]\nproject_id = \"{curseforge}\"\n"
        ),
    )
    .unwrap();
}

/// A dry run publishes one version per loader on each platform, numbered
/// `<version>+<minecraft>-<loader>`, with the dependencies of mod.toml that
/// name a project on that platform.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft"]
fn a_dry_run_describes_one_version_per_loader_and_platform() {
    let (_tmp, root) = init("fabric,neoforge", "both-optional");
    write_page(&root, "AbCd1234", "123456");
    let text = fs::read_to_string(root.join("mod.toml")).unwrap();
    fs::write(
        root.join("mod.toml"),
        format!(
            "{text}
[[dependencies]]
id = \"sodium\"
type = \"optional\"
loaders = [\"fabric\"]
modrinth = \"sodium\"
curseforge = \"sodium\"

[[dependencies]]
id = \"jei\"
type = \"required\"
loaders = [\"neoforge\"]
curseforge = \"jei\"

[[dependencies]]
id = \"oldmod\"
type = \"incompatible\"
modrinth = \"old-mod\"
"
        ),
    )
    .unwrap();
    fs::write(root.join("changelogs/1.2.0-beta.1.md"), "- First beta.\n").unwrap();

    gradle_ok(
        &root,
        &["publishMods", "-Pversion=1.2.0-beta.1", "-PdryRun=true"],
    );

    for (loader, modrinth_deps, curseforge_deps) in [
        (
            "fabric",
            &[
                "fabric-api:REQUIRED",
                "sodium:OPTIONAL",
                "old-mod:INCOMPATIBLE",
            ][..],
            &["fabric-api:REQUIRED", "sodium:OPTIONAL"][..],
        ),
        (
            "neoforge",
            &["old-mod:INCOMPATIBLE"][..],
            &["jei:REQUIRED"][..],
        ),
    ] {
        // The plugin's own dry run result is there too.
        assert!(
            root.join(format!("{loader}/build/publishMods/publishModrinth.json"))
                .is_file()
        );
        for (platform, project, deps) in [
            ("modrinth", "AbCd1234", modrinth_deps),
            ("curseforge", "123456", curseforge_deps),
        ] {
            let d = dry_run(&root, loader, platform)
                .unwrap_or_else(|| panic!("no dry run of {loader} on {platform}"));
            let number = format!("1.2.0-beta.1+26.3-{loader}");
            assert_eq!(d["version"], number.as_str(), "{loader} {platform}");
            assert_eq!(d["displayName"], format!("Swift Blades {number}").as_str());
            assert_eq!(d["projectId"], project);
            assert_eq!(d["type"], "BETA");
            assert_eq!(d["loaders"], serde_json::json!([loader]));
            assert_eq!(d["gameVersions"], serde_json::json!(["26.3"]));
            assert_eq!(d["changelog"], "- First beta.\n");
            assert_eq!(
                d["file"],
                format!("swift_blades-{loader}-1.2.0-beta.1.jar").as_str()
            );
            assert_eq!(dependencies(&d), deps, "{loader} {platform}");
        }
        let modrinth = dry_run(&root, loader, "modrinth").unwrap();
        assert_eq!(modrinth["environment"], "SERVER_ONLY_CLIENT_OPTIONAL");
        let curseforge = dry_run(&root, loader, "curseforge").unwrap();
        assert_eq!(curseforge["client"], true);
        assert_eq!(curseforge["server"], true);
    }
}

/// A platform without a project id in page/page.toml is not declared, with a
/// warning; the release type follows the version.
#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft"]
fn a_platform_without_id_is_skipped_with_a_warning() {
    let (_tmp, root) = init("fabric", "client");
    write_page(&root, "AbCd1234", "");
    for (version, kind) in [("1.0.0", "STABLE"), ("1.1.0-alpha.2", "ALPHA")] {
        fs::write(root.join(format!("changelogs/{version}.md")), "Notes.\n").unwrap();
        let output = gradle(
            &root,
            &[
                "publishMods",
                &format!("-Pversion={version}"),
                "-PdryRun=true",
            ],
        );
        assert!(output.status.success());
        let log = String::from_utf8_lossy(&output.stdout);
        assert!(
            log.contains(
                "fabric: CurseForge skipped, page/page.toml has no [curseforge] project_id"
            ),
            "{log}"
        );
        assert!(!log.contains("Modrinth skipped"), "{log}");
        let modrinth = dry_run(&root, "fabric", "modrinth").unwrap();
        assert_eq!(modrinth["type"], kind, "{version}");
        assert_eq!(modrinth["environment"], "CLIENT_ONLY");
        assert!(dry_run(&root, "fabric", "curseforge").is_none());
        assert!(
            !root
                .join("fabric/build/publishMods/publishCurseforge.json")
                .exists()
        );
    }
}

/// Publishing for real needs the version's notes: a missing changelog stops
/// the build before anything is sent.
#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft"]
fn publishing_without_changelog_fails() {
    let (_tmp, root) = init("fabric", "client");
    write_page(&root, "AbCd1234", "");
    let output = gradle(&root, &["publishMods", "-Pversion=2.0.0"]);
    assert!(!output.status.success());
    let log = String::from_utf8_lossy(&output.stderr);
    assert!(
        log.contains("changelogs/2.0.0.md is missing or empty"),
        "{log}"
    );
}
