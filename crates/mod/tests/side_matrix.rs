//! The side matrix of projects made by `mod init` (`mod test`): for each side,
//! every case passes on both loaders; and each case fails on a deliberate
//! defect. They download Gradle, the loaders, Minecraft and HeadlessMC and
//! start servers and clients, so they are `#[ignore]`: CI runs them in their
//! own job (`cargo test -p mc-mod --test side_matrix -- --ignored`). They need
//! a JDK 25, and Linux or macOS.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

const PACKAGE_DIR: &str = "com/payangar/swiftblades";

/// A project made by `mod init` in `<tmp>/mod`, with both loaders and the
/// shared library.
fn init(side: &str) -> (tempfile::TempDir, PathBuf) {
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
            "--loaders=fabric,neoforge",
        ])
        .arg(format!("--side={side}"))
        .assert()
        .success();
    let root = tmp.path().join("mod");
    (tmp, root)
}

/// `mod test <args>` in `root`: its exit status and its report.
fn mod_test(root: &Path, name: &str, args: &[&str]) -> (bool, String) {
    let output = Command::cargo_bin("mod")
        .unwrap()
        .current_dir(root)
        .arg("test")
        .args(args)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    eprintln!(
        "mod test {}\n{stdout}{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    keep_runs(root, name);
    (output.status.success(), stdout)
}

/// Copies the logs of the runs of `root` under `$MOD_SIDES_ARTIFACTS/<name>`,
/// when set: CI uploads them.
fn keep_runs(root: &Path, name: &str) {
    let Some(dir) = std::env::var_os("MOD_SIDES_ARTIFACTS") else {
        return;
    };
    copy_logs(&root.join("runs/sides"), &Path::new(&dir).join(name));
}

/// Copies the text files of `from` (logs, settings), not the game's jars and
/// worlds.
fn copy_logs(from: &Path, to: &Path) {
    let Ok(entries) = fs::read_dir(from) else {
        return;
    };
    for entry in entries.map(|e| e.unwrap()) {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !matches!(name.to_str(), Some("mods" | "saves" | "world")) {
                copy_logs(&path, &to.join(&name));
            }
        } else if path
            .extension()
            .is_some_and(|e| e == "log" || e == "txt" || e == "properties")
        {
            fs::create_dir_all(to).unwrap();
            fs::copy(&path, to.join(&name)).unwrap();
        }
    }
}

/// Every case of `side` passes on both loaders.
fn passes(side: &str) {
    let (_tmp, root) = init(side);
    let (ok, report) = mod_test(&root, side, &[]);
    assert!(ok, "mod test failed:\n{report}");
    assert!(!report.contains("FAIL"), "{report}");
    for loader in ["fabric", "neoforge"] {
        assert!(report.contains(&format!("PASS {loader} ")), "{report}");
    }
}

/// `case` fails on both loaders, with `reason` in each failure.
fn fails(root: &Path, name: &str, cases: &[&str], reason: &str) {
    let mut args = Vec::new();
    for case in cases {
        args.extend(["--case", case]);
    }
    let (ok, report) = mod_test(root, name, &args);
    assert!(!ok, "mod test passed:\n{report}");
    for loader in ["fabric", "neoforge"] {
        for case in cases {
            let failure = failure(&report, loader, case);
            assert!(
                failure.contains(reason),
                "`{reason}` missing from:\n{failure}"
            );
        }
    }
}

/// The failure of `case` on `loader` in `report`: its `FAIL` line and the
/// reason under it, up to the next case.
fn failure<'a>(report: &'a str, loader: &str, case: &str) -> &'a str {
    let line = format!("FAIL {loader} {case}: ");
    let Some(start) = report.find(&line) else {
        panic!("no `{line}` in:\n{report}");
    };
    let failure = &report[start..];
    let end = [failure[1..].find("\nPASS "), failure[1..].find("\nFAIL ")]
        .into_iter()
        .flatten()
        .min();
    &failure[..end.map_or(failure.len(), |e| e + 2)]
}

#[test]
fn a_failure_ends_at_the_next_case() {
    let report = "FAIL fabric a: first\n  reason A\n\
                  FAIL fabric b: second\n  reason B\n\
                  FAIL neoforge a: first\n  reason C\n";
    assert_eq!(
        failure(report, "fabric", "a"),
        "FAIL fabric a: first\n  reason A\n"
    );
    assert_eq!(
        failure(report, "neoforge", "a"),
        "FAIL neoforge a: first\n  reason C\n"
    );
    let report =
        "PASS fabric a: first\nFAIL fabric b: second\n  reason B\nPASS neoforge a: first\n";
    assert_eq!(
        failure(report, "fabric", "b"),
        "FAIL fabric b: second\n  reason B\n"
    );
}

fn edit(path: &Path, from: &str, to: &str) {
    let text = fs::read_to_string(path).unwrap();
    assert!(
        text.contains(from),
        "`{from}` missing from {}",
        path.display()
    );
    fs::write(path, text.replacen(from, to, 1)).unwrap();
}

fn common_entrypoint(root: &Path) -> PathBuf {
    root.join("common/src/main/java")
        .join(PACKAGE_DIR)
        .join("CommonEntrypoint.java")
}

/// The shared code reaches a client class, which a dedicated server lacks.
fn touch_a_client_class(root: &Path) {
    edit(
        &common_entrypoint(root),
        "public static void init() {}",
        "public static void init() {\n        try {\n            Class.forName(\"net.minecraft.client.Minecraft\");\n        } catch (ClassNotFoundException e) {\n            throw new IllegalStateException(\"deliberate defect\", e);\n        }\n    }",
    );
}

#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and HeadlessMC, starts servers and clients"]
fn client_mod_passes() {
    passes("client");
}

#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and HeadlessMC, starts servers and clients"]
fn server_mod_passes() {
    passes("server");
}

#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and HeadlessMC, starts servers and clients"]
fn both_required_mod_passes() {
    passes("both-required");
}

#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and HeadlessMC, starts servers and clients"]
fn both_optional_mod_passes() {
    passes("both-optional");
}

/// server-starts: shared code that reaches a client class crashes the
/// dedicated server.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft, starts servers"]
fn defect_server_starts_client_code_in_shared_code() {
    let (_tmp, root) = init("server");
    touch_a_client_class(&root);
    fails(
        &root,
        "defect-server-starts",
        &["server-starts"],
        "the server stopped",
    );
}

/// server-holds-jar: a client-only mod whose shared code reaches a client
/// class breaks a server that loads it. NeoForge loads the jar of a
/// client-only mod; Fabric skips it by its `environment`, so the defect there
/// is a socle that would lose that field.
#[test]
#[ignore = "downloads Gradle, the loaders and Minecraft, starts servers"]
fn defect_server_holds_jar_client_code_in_shared_code() {
    let (_tmp, root) = init("client");
    touch_a_client_class(&root);
    edit(
        &root.join("build-logic/templates/fabric.mod.json"),
        "\"environment\": \"${environment}\"",
        "\"environment\": \"*\"",
    );
    fails(
        &root,
        "defect-server-holds-jar",
        &["server-holds-jar"],
        "the server stopped",
    );
}

/// vanilla-client-joins: a server-only mod that requires its channel on the
/// client refuses a vanilla client.
#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and HeadlessMC, starts servers and clients"]
fn defect_vanilla_client_joins_server_mod_requires_the_client() {
    let (_tmp, root) = init("server");
    edit(
        &common_entrypoint(&root),
        "ModSide.SERVER",
        "ModSide.BOTH_REQUIRED",
    );
    fails(
        &root,
        "defect-vanilla-client-joins",
        &["vanilla-client-joins"],
        "the server refused the client",
    );
}

/// vanilla-client-refused: a mod required on both sides whose code declares
/// the other side optional lets a vanilla client in.
#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and HeadlessMC, starts servers and clients"]
fn defect_vanilla_client_refused_no_side_check() {
    let (_tmp, root) = init("both-required");
    edit(
        &common_entrypoint(&root),
        "ModSide.BOTH_REQUIRED",
        "ModSide.BOTH_OPTIONAL",
    );
    fails(
        &root,
        "defect-vanilla-client-refused",
        &["vanilla-client-refused"],
        "the client joined",
    );
}

/// modded-client-joins and modded-client-joins-unmodded-server: a client
/// whose mod crashes at startup never reaches the server. The crash is seen
/// from the client's crash line, even when the client does not exit.
#[test]
#[ignore = "downloads Gradle, the loaders, Minecraft and HeadlessMC, starts servers and clients"]
fn defect_modded_client_crashes() {
    let (_tmp, root) = init("both-optional");
    edit(
        &root
            .join("common/src/client/java")
            .join(PACKAGE_DIR)
            .join("client/ClientEntrypoint.java"),
        "ConfigScreens.register(CommonEntrypoint.SCHEMA, SETTINGS);",
        "throw new IllegalStateException(\"deliberate defect\");",
    );
    fails(
        &root,
        "defect-modded-client",
        &["modded-client-joins", "modded-client-joins-unmodded-server"],
        "the client crashed (crash report: ",
    );
}
