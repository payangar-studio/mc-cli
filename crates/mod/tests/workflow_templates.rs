//! Lint of the workflow templates of the mod socle: every generated mod is a
//! public repository, so each workflow it gets is hardened.
//!
//! - no `pull_request_target` nor `workflow_run` trigger;
//! - one run per push: with `pull_request`, `push` runs only on the version
//!   branches (`26.3`, `1.21.10`). This holds while no pull request is opened
//!   from a version branch: each push to it would run both triggers;
//! - no `mod test`: the side matrix runs locally;
//! - every action pinned by commit SHA with its version (the repository's
//!   `scripts/check-workflow-pins.sh`);
//! - no permission at the workflow level: each job states its own;
//! - `id-token: write` only in a workflow run by tags (and by hand);
//! - no `github.event` nor `github.head_ref` value in a script (`run`, or the
//!   `script` input of `actions/github-script`): they go through `env`.
//!
//! The lint runs on the templates, and on fixtures: one defect per rule, and
//! the closest legitimate cases.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use yaml_rust2::{Yaml, YamlLoader};

/// The `push` branch filter of a version branch, named after its Minecraft
/// version.
const VERSION_BRANCHES: [&str; 2] = ["[0-9]+.[0-9]+", "[0-9]+.[0-9]+.[0-9]+"];

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// What is wrong with the workflow at `path`, one line each.
fn lint(path: &Path) -> Vec<String> {
    let text = fs::read_to_string(path).unwrap();
    let docs =
        YamlLoader::load_from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let doc = &docs[0];
    let mut errors = Vec::new();

    let triggers = keys(&doc["on"]);
    for forbidden in ["pull_request_target", "workflow_run"] {
        if triggers.iter().any(|t| t == forbidden) {
            errors.push(format!(
                "trigger: `{forbidden}` runs with the repository's secrets on code it did not review"
            ));
        }
    }

    let push = &doc["on"]["push"];
    let push_on_branches = triggers.iter().any(|t| t == "push")
        && !(push["tags"].as_vec().is_some()
            && push["branches"].is_badvalue()
            && push["branches-ignore"].is_badvalue());
    let only_version_branches = push["branches-ignore"].is_badvalue()
        && push["branches"].as_vec().is_some_and(|branches| {
            !branches.is_empty()
                && branches
                    .iter()
                    .all(|b| b.as_str().is_some_and(|b| VERSION_BRANCHES.contains(&b)))
        });
    if push_on_branches && triggers.iter().any(|t| t == "pull_request") && !only_version_branches {
        errors.push(format!(
            "trigger: `push` on branches other than the version branches runs twice for each push to a pull request; limit it to `branches: {VERSION_BRANCHES:?}`"
        ));
    }

    let top = &doc["permissions"];
    if !top.is_badvalue() && keys(top).len() + usize::from(top.as_str().is_some()) > 0 {
        errors.push("permissions: the workflow grants permissions; set `permissions: {}` and give each job its own".into());
    }

    let tags_only = triggers
        .iter()
        .all(|t| t == "workflow_dispatch" || t == "push")
        && doc["on"]["push"]["tags"].as_vec().is_some()
        && doc["on"]["push"]["branches"].is_badvalue()
        && doc["on"]["push"]["branches-ignore"].is_badvalue();
    let jobs = doc["jobs"].as_hash().expect("a workflow has jobs");
    for (name, job) in jobs {
        let name = name.as_str().unwrap_or("?");
        let permissions = &job["permissions"];
        if permissions.as_hash().is_none() {
            errors.push(format!(
                "permissions: job `{name}` does not state its permissions"
            ));
        }
        if permissions["id-token"].as_str() == Some("write") && !tags_only {
            errors.push(format!(
                "permissions: job `{name}` has `id-token: write` in a workflow not limited to tags"
            ));
        }
        for step in job["steps"].as_vec().into_iter().flatten() {
            if step["run"].as_str().is_some_and(runs_mod_test) {
                errors.push(format!(
                    "side matrix: job `{name}` runs `mod test`, which starts servers and clients for about 20 minutes; it runs locally"
                ));
            }
            let scripts = [&step["run"], &step["with"]["script"]];
            for expression in scripts
                .into_iter()
                .filter_map(Yaml::as_str)
                .flat_map(|script| script.split("${{").skip(1))
            {
                let expression = expression.split("}}").next().unwrap_or_default();
                if expression.contains("github.event") || expression.contains("github.head_ref") {
                    errors.push(format!(
                        "injection: job `{name}` writes `${{{{{expression}}}}}` into a script; pass it through `env`"
                    ));
                }
            }
        }
    }

    let pins = Command::new("bash")
        .arg(manifest_dir().join("../../scripts/check-workflow-pins.sh"))
        .arg(path)
        .output()
        .expect("bash runs");
    if !pins.status.success() {
        let out = String::from_utf8_lossy(&pins.stdout);
        errors.extend(out.lines().map(|l| format!("pin: {l}")));
    }
    errors
}

/// Whether a `run` script runs `mod test`, called by name or by path, in any
/// command of a line; comments aside.
fn runs_mod_test(script: &str) -> bool {
    script.lines().any(|line| {
        let words: Vec<_> = line
            .split(|c: char| c.is_whitespace() || ";&|()`".contains(c))
            .map(|word| word.trim_matches(['"', '\'']))
            .filter(|word| !word.is_empty())
            .take_while(|word| !word.starts_with('#'))
            .collect();
        words
            .windows(2)
            .any(|w| w[0].rsplit('/').next() == Some("mod") && w[1] == "test")
    })
}

/// Keys of a mapping, or the names of a trigger list or a single trigger.
fn keys(yaml: &Yaml) -> Vec<String> {
    match yaml {
        Yaml::Hash(hash) => hash
            .keys()
            .filter_map(|k| k.as_str().map(str::to_owned))
            .collect(),
        Yaml::Array(items) => items
            .iter()
            .filter_map(|k| k.as_str().map(str::to_owned))
            .collect(),
        Yaml::String(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// Every workflow of the mod socle's templates.
fn templates() -> Vec<PathBuf> {
    let dir = manifest_dir().join("templates/socle/.github/workflows");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files
}

fn fixture(name: &str) -> PathBuf {
    manifest_dir().join("tests/fixtures/workflows").join(name)
}

#[test]
fn every_template_passes() {
    let files = templates();
    assert!(!files.is_empty());
    for file in files {
        let ext = file.extension().and_then(|e| e.to_str());
        assert_eq!(ext, Some("yml"), "{}: name workflows *.yml", file.display());
        let errors = lint(&file);
        assert!(
            errors.is_empty(),
            "{}:\n{}",
            file.display(),
            errors.join("\n")
        );
    }
}

#[test]
fn each_defect_fails_on_its_rule() {
    for (name, expected) in [
        ("pull-request-target.yml", "trigger: `pull_request_target`"),
        ("workflow-run.yml", "trigger: `workflow_run`"),
        ("unpinned-action.yml", "pin: "),
        (
            "no-job-permissions.yml",
            "job `job` does not state its permissions",
        ),
        (
            "workflow-permissions.yml",
            "the workflow grants permissions",
        ),
        (
            "id-token-outside-tags.yml",
            "`id-token: write` in a workflow not limited to tags",
        ),
        (
            "id-token-on-branches-ignore.yml",
            "`id-token: write` in a workflow not limited to tags",
        ),
        (
            "event-in-run.yml",
            "injection: job `job` writes `${{ github.event.pull_request.title }}`",
        ),
        (
            "event-in-script.yml",
            "injection: job `job` writes `${{ github.event.pull_request.title }}`",
        ),
        (
            "push-and-pull-request.yml",
            "trigger: `push` on branches other than the version branches",
        ),
        (
            "push-on-version-prefix.yml",
            "trigger: `push` on branches other than the version branches",
        ),
        ("mod-test.yml", "side matrix: job `job` runs `mod test`"),
    ] {
        let errors = lint(&fixture(name));
        assert_eq!(errors.len(), 1, "{name}: {errors:?}");
        assert!(errors[0].contains(expected), "{name}: {errors:?}");
    }
}

#[test]
fn the_closest_legitimate_cases_pass() {
    for name in [
        "event-through-env.yml",
        "tag-job.yml",
        "version-branches.yml",
    ] {
        let errors = lint(&fixture(name));
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
}

#[test]
fn mod_test_is_found_in_any_shell_form() {
    for script in [
        "mod test",
        "mod test --loader fabric",
        "./gradlew build && mod test",
        "./gradlew build&&mod test",
        "mod test; echo done",
        "echo \"$(mod test)\"",
        "\"$RUNNER_TEMP/mc-cli/bin/mod\" test --loader fabric",
        "~/.cargo/bin/mod test",
    ] {
        assert!(runs_mod_test(script), "{script}");
    }
    for script in [
        "mod run --profile sodium",
        "# run mod test locally, before a release",
        "echo modtest",
        "test -f mod.toml",
    ] {
        assert!(!runs_mod_test(script), "{script}");
    }
}
