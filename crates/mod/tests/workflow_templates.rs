//! Lint of the workflow templates of the mod socle: every generated mod is a
//! public repository, so each workflow it gets is hardened.
//!
//! - no `pull_request_target` nor `workflow_run` trigger;
//! - every action pinned by commit SHA with its version (the repository's
//!   `scripts/check-workflow-pins.sh`);
//! - no permission at the workflow level: each job states its own;
//! - `id-token: write` only in a workflow run by tags (and by hand);
//! - no `github.event` nor `github.head_ref` value in a `run` script: they go
//!   through `env`.
//!
//! The lint runs on the templates, and on fixtures: one defect per rule, and
//! the closest legitimate cases.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use yaml_rust2::{Yaml, YamlLoader};

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

    let top = &doc["permissions"];
    if !top.is_badvalue() && keys(top).len() + usize::from(top.as_str().is_some()) > 0 {
        errors.push("permissions: the workflow grants permissions; set `permissions: {}` and give each job its own".into());
    }

    let tags_only = triggers
        .iter()
        .all(|t| t == "workflow_dispatch" || t == "push")
        && doc["on"]["push"]["tags"].as_vec().is_some()
        && doc["on"]["push"]["branches"].is_badvalue();
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
            let Some(run) = step["run"].as_str() else {
                continue;
            };
            for expression in run.split("${{").skip(1) {
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
            "event-in-run.yml",
            "injection: job `job` writes `${{ github.event.pull_request.title }}`",
        ),
    ] {
        let errors = lint(&fixture(name));
        assert_eq!(errors.len(), 1, "{name}: {errors:?}");
        assert!(errors[0].contains(expected), "{name}: {errors:?}");
    }
}

#[test]
fn the_closest_legitimate_cases_pass() {
    for name in ["event-through-env.yml", "tag-job.yml"] {
        let errors = lint(&fixture(name));
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
}
