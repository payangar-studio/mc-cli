//! Builds of projects made by `mod init`, with their own Gradle wrapper. They
//! download Gradle, the loaders and Minecraft, so they are `#[ignore]`: CI runs
//! them in their own job (`cargo test -p mc-mod --test gradle_build --
//! --ignored`). They need a JDK 25 (`JAVA_HOME` or on the path).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};

use assert_cmd::Command;

const PACKAGE_DIR: &str = "com/payangar/swiftblades";

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
            "--package=com.payangar.swiftblades",
            "--license=MIT",
            "--mc-version=26.3",
        ])
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
        "common/src/main/java/{PACKAGE_DIR}/SplitFixture.java"
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
        "common/src/client/java/{PACKAGE_DIR}/SplitFixture.java"
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

/// Copies the shared library and its tests into the generated mod `root`,
/// with `common/` of the templates as `common/` of the mod, rewriting the
/// template package to the mod's vendor package. Until `mod init` copies the
/// library itself, this test does it, and adds JUnit to `common`.
fn copy_library(root: &Path) {
    let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/library");
    let package = "com.payangar.swiftblades.vendor";
    let template_dir = LIBRARY_PACKAGE.replace('.', "/");
    let package_dir = package.replace('.', "/");
    let mut copied = 0;
    for entry in walk(&templates) {
        let relative = entry.strip_prefix(&templates).unwrap().to_str().unwrap();
        let target = root.join(relative.replace(&template_dir, &package_dir));
        let text = fs::read_to_string(&entry).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, text.replace(LIBRARY_PACKAGE, package)).unwrap();
        copied += 1;
    }
    assert!(copied > 0, "no library file in {}", templates.display());

    let build = root.join("common/build.gradle.kts");
    let mut script = fs::read_to_string(&build).unwrap();
    script.push_str(
        r#"
dependencies {
    "testImplementation"(platform("org.junit:junit-bom:6.1.3"))
    "testImplementation"("org.junit.jupiter:junit-jupiter")
    "testRuntimeOnly"("org.junit.platform:junit-platform-launcher")
}

tasks.named<Test>("test") {
    useJUnitPlatform()
}
"#,
    );
    fs::write(&build, script).unwrap();
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

/// The shared library compiles in a generated mod for both loaders, and its
/// unit tests pass there.
#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and JUnit"]
fn library_tests_pass_in_a_generated_mod() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    copy_library(&root);
    gradle_ok(&root, &["build"]);
    assert!(jar(&root, "fabric").is_file());
    assert!(jar(&root, "neoforge").is_file());

    // Each test class ran: a green build with no test run proves nothing.
    let results = root.join("common/build/test-results/test");
    for class in ["ConfigSchemaTest", "ConfigFileTest"] {
        let report = results.join(format!(
            "TEST-com.payangar.swiftblades.vendor.config.{class}.xml"
        ));
        let text =
            fs::read_to_string(&report).unwrap_or_else(|e| panic!("{}: {e}", report.display()));
        assert!(!text.contains("tests=\"0\""), "{class} ran no test");
        assert!(text.contains("failures=\"0\""), "{text}");
    }
}
