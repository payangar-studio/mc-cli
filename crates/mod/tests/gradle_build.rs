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

        let neoforge = neoforge_mods_toml(&root);
        assert_eq!(neoforge["mods"][0]["modId"].as_str(), Some("swift_blades"));
        assert_eq!(neoforge["license"].as_str(), Some("MIT"));
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

/// A line the library's client gametest logs on the dedicated server once an
/// operator's change is applied, which only that gametest makes.
const LIBRARY_GAMETEST_MARK: &str =
    "changed server settings of vendor_network_test: {\"range\":16}";

/// The library's client gametest ran in `:fabric:runClientGameTest`, up to
/// the operator's change: the socle's own client gametest passes that run as
/// well, so a green build does not show the library's ran.
fn assert_library_gametest_ran(root: &Path) {
    let log = root.join("fabric/build/run/clientGameTest/logs/latest.log");
    let text = fs::read_to_string(&log).unwrap_or_else(|e| panic!("{}: {e}", log.display()));
    assert!(
        text.contains(LIBRARY_GAMETEST_MARK),
        "the library's client gametest did not run: `{LIBRARY_GAMETEST_MARK}` missing from {}",
        log.display()
    );
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

    private static final ConfigSchema SCHEMA = schema();

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
