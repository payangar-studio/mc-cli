//! The test pyramid of projects made by `mod init`: each layer's example
//! passes, and each guard catches its defect. They download Gradle, the
//! loaders and Minecraft and start the game, so they are `#[ignore]`: CI runs
//! them in their own job (`cargo test -p mc-mod --test test_pyramid --
//! --ignored`). They need a JDK 25, and Xvfb for the client runs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};

use assert_cmd::Command;

mod library;

const PACKAGE_DIR: &str = "com/payangar/swiftblades";

/// A project made by `mod init` in `<tmp>/mod`.
fn init(loaders: &str, side: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("mod")
        .unwrap()
        .current_dir(tmp.path())
        .env("XDG_CONFIG_HOME", tmp.path().join(".machine-config"))
        .args([
            "init",
            "mod",
            "--id=swift_blades",
            "--name=Swift Blades",
            "--package=com.payangar.swiftblades",
            "--license=MIT",
            "--mc-version=26.3",
        ])
        .arg(format!("--loaders={loaders}"))
        .arg(format!("--side={side}"))
        .assert()
        .success();
    let root = tmp.path().join("mod");
    (tmp, root)
}

/// `./gradlew <args>` in `root`, its output echoed for the CI log.
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

fn log(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// `./gradlew <args>` fails, with `message` in its output.
fn gradle_fails(root: &Path, args: &[&str], message: &str) {
    let output = gradle(root, args);
    assert!(
        !output.status.success(),
        "./gradlew {} passed",
        args.join(" ")
    );
    let log = log(&output);
    assert!(log.contains(message), "`{message}` missing from the output");
}

/// Copies the renders and logs of the game runs of `root` under
/// `$MOD_TESTS_ARTIFACTS/<name>`, when set: CI uploads them, and a render of
/// CI is what becomes the reference of the example screenshot test.
fn keep_runs(root: &Path, name: &str) {
    let Some(dir) = std::env::var_os("MOD_TESTS_ARTIFACTS") else {
        return;
    };
    for loader in ["fabric", "neoforge"] {
        let runs = root.join(loader).join("build/run");
        let Ok(entries) = fs::read_dir(&runs) else {
            continue;
        };
        for run in entries.map(|e| e.unwrap().path()) {
            for kept in ["screenshots", "logs", "crash-reports"] {
                let from = run.join(kept);
                if from.is_dir() {
                    let to = Path::new(&dir)
                        .join(name)
                        .join(loader)
                        .join(run.file_name().unwrap())
                        .join(kept);
                    copy_dir(&from, &to);
                }
            }
        }
    }
}

/// The JUnit tests of the shared library copied into `subproject` of a mod
/// with both loaders: one per `@Test` annotation in the Java sources of its
/// `src/test/java`.
fn library_tests(subproject: &str) -> usize {
    fn count(dir: &Path) -> usize {
        let Ok(entries) = fs::read_dir(dir) else {
            return 0;
        };
        entries
            .map(|e| e.unwrap().path())
            .map(|path| {
                if path.is_dir() {
                    return count(&path);
                }
                if path.extension().is_none_or(|e| e != "java") {
                    return 0;
                }
                let text = fs::read_to_string(&path).unwrap();
                assert!(
                    ["@ParameterizedTest", "@RepeatedTest", "@TestFactory"]
                        .iter()
                        .all(|a| !text.contains(a)),
                    "{}: count its tests here",
                    path.display()
                );
                text.lines().filter(|l| annotated_test(l)).count()
            })
            .sum()
    }
    count(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("templates/library")
            .join(subproject)
            .join("src/test/java"),
    )
}

#[test]
fn library_tests_are_counted_by_their_annotation() {
    assert!(library_tests("common") > 0);
    assert!(annotated_test("    @Test"));
    assert!(annotated_test("@Test @Timeout(5)"));
    assert!(!annotated_test("@TestInstance(Lifecycle.PER_CLASS)"));
    assert!(!annotated_test("// @Test"));
}

/// Whether `line` starts with the `@Test` annotation, alone or followed by
/// others (`@Test @Timeout(5)`).
fn annotated_test(line: &str) -> bool {
    line.trim_start()
        .strip_prefix("@Test")
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().map(|e| e.unwrap()) {
        let path = entry.path();
        if path.is_dir() {
            copy_dir(&path, &to.join(entry.file_name()));
        } else {
            fs::copy(&path, to.join(entry.file_name())).unwrap();
        }
    }
}

fn edit(path: &Path, from: &str, to: &str) {
    let text = fs::read_to_string(path).unwrap();
    assert!(
        text.contains(from),
        "`{from}` missing from {}",
        path.display()
    );
    fs::write(path, text.replace(from, to)).unwrap();
}

/// The end-to-end test of the socle: a mod with both loaders builds, and every
/// layer's example passes: unit tests, NeoForge's integration tests, both
/// loaders' server gametests, Fabric's client gametests against the reference
/// render (dev and production clients), NeoForge's self-tests, along with the
/// shared library's tests.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft, starts the game"]
fn every_layer_passes() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    let output = gradle(&root, &["build", "clientCheck"]);
    keep_runs(&root, "every_layer_passes");
    assert!(
        output.status.success(),
        "./gradlew build clientCheck failed"
    );
    let log = log(&output);
    for (task, tests) in [
        (":common:test", 1 + library_tests("common")),
        (":fabric:test", 1 + library_tests("fabric")),
        (":fabric:runGameTest", 1),
        (":neoforge:test", 2 + library_tests("neoforge")),
        (":neoforge:runGameTestServer", 1),
    ] {
        assert!(
            log.contains(&format!("{task} ran {tests} test(s).")),
            "{task} did not run {tests} test(s)"
        );
    }
    for task in [
        ":fabric:runClientGameTest",
        ":fabric:runProductionClientGameTest",
        ":neoforge:runServerSelfTest",
        ":neoforge:runClientSelfTest",
    ] {
        assert!(
            log.contains(&format!("> Task {task}\n")),
            "{task} did not run"
        );
    }
    // The comparison saved the render it took: the CI artifact to promote.
    assert!(
        root.join("fabric/build/run/clientGameTest/screenshots/example_world.png")
            .is_file()
    );
    assert!(
        root.join("fabric/build/run/productionClientGameTest/screenshots/example_world.png")
            .is_file()
    );
}

/// A test source set without tests fails its task, with what to do.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft"]
fn a_test_task_without_tests_fails() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    fs::remove_dir_all(root.join("common/src/test")).unwrap();
    gradle_fails(
        &root,
        &[":common:test"],
        ":common:test ran no test. Write at least one test in common/src/test/java.",
    );
}

/// A failing server gametest fails the run, on each loader.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft, starts the game"]
fn a_failing_server_gametest_fails() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    for loader in ["fabric", "neoforge"] {
        edit(
            &root.join(format!(
                "{loader}/src/gametest/java/{PACKAGE_DIR}/gametest/ExampleServerGameTest.java"
            )),
            "helper.assertBlockPresent(Blocks.DIAMOND_BLOCK, 1, 1, 1);",
            "helper.assertBlockPresent(Blocks.GOLD_BLOCK, 1, 1, 1);",
        );
    }
    gradle_fails(&root, &[":fabric:runGameTest"], "required tests failed");
    gradle_fails(
        &root,
        &[":neoforge:runGameTestServer"],
        "required tests failed",
    );
}

/// A server gametest run that registers no test fails, on each loader.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft, starts the game"]
fn a_server_gametest_run_without_tests_fails() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    edit(
        &root.join("fabric/src/gametest/resources/fabric.mod.json"),
        "\"fabric-gametest\": [\"com.payangar.swiftblades.gametest.ExampleServerGameTest\"],",
        "\"fabric-gametest\": [],",
    );
    edit(
        &root.join(format!(
            "neoforge/src/gametest/java/{PACKAGE_DIR}/gametest/ExampleServerGameTest.java"
        )),
        "modBus.addListener(ExampleServerGameTest::register);",
        "",
    );
    gradle_fails(
        &root,
        &[":fabric:runGameTest"],
        ":fabric:runGameTest ran no test.",
    );
    gradle_fails(
        &root,
        &[":neoforge:runGameTestServer"],
        ":neoforge:runGameTestServer ran no test.",
    );
}

/// A client gametest run without a client gametest fails, on the dev
/// client and as in production.
#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft, starts the game"]
fn a_client_gametest_run_without_tests_fails() {
    let (_tmp, root) = init("fabric", "both-required");
    edit(
        &root.join("fabric/src/gametest/resources/fabric.mod.json"),
        "\"fabric-client-gametest\": [\"com.payangar.swiftblades.gametest.ExampleClientGameTest\"]",
        "\"fabric-client-gametest\": []",
    );
    for task in [
        ":fabric:runClientGameTest",
        ":fabric:runProductionClientGameTest",
    ] {
        gradle_fails(
            &root,
            &[task],
            "No tests found for the specified mod IDs: [swift_blades_gametest]",
        );
    }
}

/// A render that differs from the reference fails the screenshot test, and a
/// missing reference is an error, never a new reference.
#[test]
#[ignore = "downloads Gradle, Fabric and Minecraft, starts the game"]
fn a_changed_render_fails_the_screenshot_test() {
    let (_tmp, root) = init("fabric", "both-required");
    let test = root.join(format!(
        "fabric/src/gametest/java/{PACKAGE_DIR}/gametest/ExampleClientGameTest.java"
    ));
    edit(
        &test,
        "world.getConnection().waitForChunksRender();",
        "world.getConnection().waitForChunksRender();\n            \
         context.runOnClient(client -> client.options.fov().set(30));",
    );
    gradle_fails(
        &root,
        &[":fabric:runClientGameTest"],
        "Screenshot does not contain template 'example_world'",
    );
    keep_runs(&root, "a_changed_render_fails_the_screenshot_test");

    let reference = root.join("fabric/src/gametest/resources/templates/example_world.png");
    fs::remove_file(&reference).unwrap();
    gradle_fails(
        &root,
        &[":fabric:runClientGameTest"],
        "Template image does not exist",
    );
    assert!(!reference.exists(), "the run wrote a reference");
}

/// The shared library's config screens on a Fabric client
/// (`ConfigScreenClientGameTest`, which the test adds to the mod): the generated
/// screen and the widget states match their reference renders, and settings
/// changed with the mouse and the keyboard reach the JSON file.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft, starts the game"]
fn library_config_screens_pass_on_a_client() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    library::copy_gametests(&root, "com.payangar.swiftblades");
    let output = gradle(&root, &[":fabric:runClientGameTest"]);
    keep_runs(&root, "library_config_screens_pass_on_a_client");
    assert!(
        output.status.success(),
        "./gradlew :fabric:runClientGameTest failed"
    );
    // The comparisons saved the renders they took: the CI artifact to promote.
    for name in [
        "vendor_config_screen",
        "vendor_widget_states",
        "vendor_dropdown_open",
    ] {
        assert!(
            root.join(format!(
                "fabric/build/run/clientGameTest/screenshots/{name}.png"
            ))
            .is_file(),
            "{name}.png was not saved"
        );
    }
}

/// The config screen of a mod made by `mod init` opens from each loader's mod
/// list: from Mod Menu on Fabric, in a client gametest the test adds with Mod
/// Menu (compile-only in the mod) on its classpath; from NeoForge's mod list,
/// in a check the test adds to the client self-test, NeoForge having no client
/// gametests. Both go through the loader's own lookup, never the library's.
#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and Mod Menu, starts the game"]
fn the_config_screen_opens_from_each_mod_list() {
    let (_tmp, root) = init("fabric,neoforge", "both-required");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/config_screen");
    let add = |fixture: &str, to: String| {
        let to = root.join(to);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(fixtures.join(fixture), to).unwrap();
    };
    add(
        "ModMenuConfigScreenGameTest.java",
        format!("fabric/src/gametest/java/{PACKAGE_DIR}/gametest/ModMenuConfigScreenGameTest.java"),
    );
    add(
        "ModListConfigScreenCheck.java",
        format!(
            "neoforge/src/main/java/{PACKAGE_DIR}/neoforge/client/ModListConfigScreenCheck.java"
        ),
    );
    edit(
        &root.join("fabric/src/gametest/resources/fabric.mod.json"),
        "\"fabric-client-gametest\": [",
        "\"fabric-client-gametest\": [\"com.payangar.swiftblades.gametest.ModMenuConfigScreenGameTest\", ",
    );
    // Intransitive: the jar nests what it needs, while its Maven metadata would
    // move the pinned Fabric API modules on the gametest classpath.
    let modmenu = mc_mod::versions::of("26.3").unwrap().modmenu;
    edit(
        &root.join("fabric/build.gradle.kts"),
        "dependencies {",
        &format!(
            "dependencies {{\n    \"gametestImplementation\"(\"com.terraformersmc:modmenu:{modmenu}\") {{ isTransitive = false }}"
        ),
    );

    let output = gradle(
        &root,
        &[":fabric:runClientGameTest", ":neoforge:runClientSelfTest"],
    );
    keep_runs(&root, "the_config_screen_opens_from_each_mod_list");
    assert!(
        output.status.success(),
        "./gradlew :fabric:runClientGameTest :neoforge:runClientSelfTest failed"
    );
    let found = "Config screen from the mod list: com.payangar.swiftblades.vendor.gui.screen.GeneratedConfigScreen, showing CommonEntrypoint.SCHEMA: true";
    assert!(
        log(&output).contains(found),
        "NeoForge's mod list found no generated config screen of CommonEntrypoint.SCHEMA"
    );
}
