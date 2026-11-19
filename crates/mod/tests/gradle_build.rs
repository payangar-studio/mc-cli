//! Builds of projects made by `mod init`, with their own Gradle wrapper. They
//! download Gradle, the loaders and Minecraft, so they are `#[ignore]`: CI runs
//! them in their own job (`cargo test -p mc-mod --test gradle_build --
//! --ignored`). They need a JDK 25 (`JAVA_HOME` or on the path).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};

use assert_cmd::Command;

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
    Command::cargo_bin("mod")
        .unwrap()
        .current_dir(tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path().join(".machine-config"))
        .args([
            "init",
            "mod",
            "--id=swift_blades",
            "--license=MIT",
            "--mc-version=26.3",
        ])
        .arg(format!("--package={PACKAGE}"))
        .arg(format!("--name={name}"))
        .arg(format!("--loaders={loaders}"))
        .arg(format!("--side={side}"))
        .assert()
        .success();
    let root = tmp.path().join("mod");
    (tmp, root)
}

fn gradle(root: &Path, args: &[&str]) -> Output {
    let output = Process::new(root.join("gradlew"))
        .args(["--console=plain", "--stacktrace"])
        .args(args)
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
}

#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft"]
fn builds_with_fabric_only() {
    let (_tmp, root) = init("fabric", "client");
    assert!(!root.join("common").exists());
    gradle_ok(&root, &["build"]);
    assert!(jar(&root, "fabric").is_file());
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

#[test]
#[ignore = "downloads Gradle, NeoForge and Minecraft"]
fn builds_with_neoforge_only() {
    let (_tmp, root) = init("neoforge", "server");
    assert!(!root.join("common").exists());
    gradle_ok(&root, &["build"]);
    assert!(jar(&root, "neoforge").is_file());
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

/// The template package of the shared library (`templates/library/`).
const LIBRARY_PACKAGE: &str = "com.payangar.socle.vendor";

/// The entrypoints of the library's gametests, added to the gametest mod of
/// `fabric/`, relative to the mod's vendor package.
const LIBRARY_GAMETEST_ENTRYPOINTS: &[(&str, &str)] = &[
    ("main", "network.NetworkTestMod"),
    ("client", "network.NetworkTestModClient"),
    ("fabric-client-gametest", "network.ConfigSyncClientGameTest"),
];

/// Copies the shared library and its tests into the generated mod `root`,
/// rewriting the template package to the mod's vendor package: `common/` of
/// the templates into `shared/` of the mod (`common` with two loaders, the
/// loader's subproject with one), and `fabric/` and `neoforge/` into the
/// loader's subproject when the mod has it. Until `mod init` copies the
/// library itself, this test does it, and lists the library's gametests in
/// the gametest mod of `fabric/`.
fn copy_library(root: &Path, shared: &str) {
    let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/library");
    let package = format!("{PACKAGE}.vendor");
    let mut copied = 0;
    for entry in walk(&templates) {
        let relative = entry.strip_prefix(&templates).unwrap();
        let target = library_path(relative, shared, &package);
        let subproject = root.join(target.iter().next().unwrap());
        // The library of a loader the mod does not have; `shared` is always there.
        if !relative.starts_with("common") && !subproject.is_dir() {
            continue;
        }
        assert!(subproject.is_dir(), "{} is missing", subproject.display());
        let target = root.join(target);
        let text = fs::read_to_string(&entry).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, text.replace(LIBRARY_PACKAGE, &package)).unwrap();
        copied += 1;
    }
    assert!(copied > 0, "no library file in {}", templates.display());

    if root.join("fabric").is_dir() {
        let gametest_mod = root.join("fabric/src/gametest/resources/fabric.mod.json");
        let text = fs::read_to_string(&gametest_mod)
            .unwrap_or_else(|e| panic!("{}: {e}", gametest_mod.display()));
        let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let entrypoints = json["entrypoints"].as_object_mut().unwrap();
        for (kind, class) in LIBRARY_GAMETEST_ENTRYPOINTS {
            let list = entrypoints
                .entry(kind.to_string())
                .or_insert_with(|| serde_json::json!([]));
            list.as_array_mut()
                .unwrap()
                .push(format!("{package}.{class}").into());
        }
        fs::write(&gametest_mod, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    }
}

/// Where the library file at `relative` in the templates goes in the mod:
/// `common` replaced by `shared`, the template package directories by those
/// of `package`. Rewritten by components, whatever the path separator.
fn library_path(relative: &Path, shared: &str, package: &str) -> PathBuf {
    let parts: Vec<&str> = relative.iter().map(|p| p.to_str().unwrap()).collect();
    let template: Vec<&str> = LIBRARY_PACKAGE.split('.').collect();
    let subproject = match parts[0] {
        "common" => shared,
        "fabric" | "neoforge" => parts[0],
        other => panic!("{}: unknown subproject {other}", relative.display()),
    };
    let start = parts
        .windows(template.len())
        .position(|w| w == template.as_slice())
        .unwrap_or_else(|| panic!("{} is outside {LIBRARY_PACKAGE}", relative.display()));
    let mut path = PathBuf::from(subproject);
    path.extend(&parts[1..start]);
    path.extend(package.split('.'));
    path.extend(&parts[start + template.len()..]);
    path
}

/// Every file under `dir`, recursively.
fn walk(dir: &Path) -> Vec<PathBuf> {
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

/// The shared library compiles in a generated mod for both loaders, and its
/// tests pass there: unit tests, the server side of the sync on NeoForge
/// with the game loaded, and a Fabric client gametest on a dedicated server.
/// The client gametests need a display: Xvfb, as in CI.
#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and JUnit, and starts the game"]
fn library_tests_pass_in_a_generated_mod() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    copy_library(&root, "common");
    gradle_ok(&root, &["build", ":fabric:runClientGameTest"]);
    assert!(jar(&root, "fabric").is_file());
    assert!(jar(&root, "neoforge").is_file());
    assert_library_tests_passed(&root, "common", COMMON_LIBRARY_TESTS);
    assert_library_tests_passed(&root, "neoforge", &["network.ServerConfigTest"]);
    assert_library_gametest_ran(&root);
}

/// A single-loader mod has no `common/`: the library goes into the loader's
/// subproject, compiles and passes its tests there.
#[test]
#[ignore = "downloads Gradle, Fabric, Minecraft and JUnit"]
fn library_tests_pass_in_a_single_loader_mod() {
    let (_tmp, root) = init("fabric", "client");
    copy_library(&root, "fabric");
    gradle_ok(&root, &["build"]);
    assert!(!root.join("common").exists());
    assert!(!root.join("neoforge").exists());
    assert!(jar(&root, "fabric").is_file());
    assert_library_tests_passed(&root, "fabric", COMMON_LIBRARY_TESTS);
}
