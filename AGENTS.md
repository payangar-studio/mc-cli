# mc-cli

Rust CLIs of the Minecraft socles, in one Cargo workspace: the shared library `mc-core` (`crates/mc-core/`) and the binary crates `modpack` (`crates/modpack/`) and `mod` (`crates/mod/`, package `mc-mod`).
The design lives in `payangar-studio/environment`, branch `design`, files 00-cadrage.md, 01-socle-modpack.md and 02-socle-mod.md under docs/design; it is the source of truth for behaviour.

## Layout

`crates/mc-core/` holds what every binary shares: errors, prompts, secrets, platforms, socle mechanics, mod resolution, page model, dedicated server. Its code names no socle path of a project type and no type only one binary uses (`crates/mc-core/tests/independence.rs` checks it); the shared skills of `crates/mc-core/templates/socle/` may name each type's files.

- `crates/modpack/src/main.rs`: entry point. Parses the command line, sets up logs, builds the context, calls `cli::run`, maps errors to exit codes.
- `crates/modpack/src/lib.rs`: module registry, one `pub mod` line per module.
- `crates/modpack/src/cli/mod.rs`: the clap command tree and the dispatch to handlers.
- `crates/modpack/src/cli/<command>.rs`: one file per command (`variant load` is `crates/modpack/src/cli/variant_load.rs`), holding its `Args` and its `run(args, ctx)` handler.
- `crates/mc-core/src/prompt.rs`: "ask, or read the argument". Every value a command needs goes through it, so that every command can run without a terminal.
- `crates/mc-core/src/error.rs`: error model. Every error says what happened and what to do. Exit codes: 0 success, 1 internal error, 2 usage error (missing argument), 3 blocked (drift or file edited by hand, overridable with `--force`; also a failed `modpack test` condition or a platform `modpack pack` cannot build for).
- `crates/modpack/src/context.rs`: context passed to handlers (repository of any type found by walking up to its marker, from `crates/mc-core/src/project.rs`; prompt, verbosity).
- `crates/modpack/src/socle/`: the modpack socle (`SOCLE`: directory, templates, markers) over the mechanics of `crates/mc-core/src/socle/`, and the socle version check that `Context::require_root` runs for every command working in a repository.
- `crates/modpack/src/conditions/`: the checks behind `modpack test`. Static ones read the manifest; `server-starts` installs the loader's server once into the machine cache (`<cache dir>/modpack/servers`) through `crates/mc-core/src/server/`, then starts it with the variant's jars and config until `Done`.
- `crates/modpack/templates/`: templates the binary embeds. `socle/`: files the socle owns, at their path in the pack, rewritten by `update`. `blocks/`: blocks between markers in `AGENTS.md` and `.gitignore`. `project/`: the pack skeleton, written once by `init`. Adding a file to `socle/` or `project/` needs no code change; each issue adds its templates under its own path.
- `crates/mc-core/templates/socle/`: files every socle owns whatever the project type (skills shared by the binaries, named `mc-*`), laid down with each socle's own files.
- `crates/modpack/tests/`: integration tests of the binary; `crates/modpack/tests/fixtures/`: their fixtures.
- `crates/mod/` follows the same layout. `crates/mod/templates/socle/` is the generated mod's Gradle build and harness, and its workflows pass `crates/mod/tests/workflow_templates.rs`; `crates/mod/src/socle/skeleton.rs` lays out the mod's own files. `crates/mod/tests/gradle_build.rs` builds generated mods: `#[ignore]`, run by the CI job `mod-build`, needs a JDK 25, and Xvfb for the library's client gametests. `crates/mod/tests/test_pyramid.rs` runs their test layers and guards: `#[ignore]`, CI job `mod-tests`, also needs Xvfb.
- `crates/mod/templates/library/`: the shared library copied into each mod, laid out by subproject and source set, in the template package `com.payangar.socle.vendor` that the copy rewrites. Its code rules are in that package's `package-info.java`; its strings are in `lang/<locale>.json`, keyed without the `<modid>.vendor.` prefix the copy adds.

## Who edits what

Each issue owns its command handler (`crates/modpack/src/cli/<command>.rs`, including its `Args`) and the modules it creates.

- `crates/modpack/src/main.rs`, `crates/modpack/src/cli/mod.rs`, `crates/modpack/src/context.rs`, and `crates/mc-core/src/prompt.rs` and `crates/mc-core/src/error.rs`, only change through an issue dedicated to them.
- `crates/mc-core/src/lib.rs`, `crates/modpack/src/lib.rs` and every `Cargo.toml` are append-only: add a module line or a dependency, never change the others. Merge conflicts on them are resolved by keeping both sides.

## Conventions

- Logs go to stderr through `tracing`; command output goes to stdout.
- Machine locations (caches, data, config) come from the `directories` crate, never from hard-coded Linux paths, so that Windows follows without a rewrite.
- User-facing messages are in English.
- Until a modpack is published with this CLI, a format change (repository files, instance state) ships without migration or backward compatibility; migrate the test pack by hand.

## Tests

- Unit tests live in each module (`#[cfg(test)] mod tests`).
- Integration tests of a binary live in `crates/<crate>/tests/` and drive it with `assert_cmd`. Fixtures go in `crates/<crate>/tests/fixtures/`.
- A test writes an executable file with `write_executable` (`crates/modpack/tests/common/mod.rs`, included with `#[path]` from `src/`); `scripts/check-test-executables.sh` enforces it in CI.
- No network call in default tests. A test that needs the network is `#[ignore]` and run explicitly (`cargo test -- --ignored`); a CurseForge one also skips itself when `CURSEFORGE_API_KEY` is not set.
- Locally, run only the tests of the code you touched (`cargo test <filter>`), plus `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`. The full suite runs in CI on every push and pull request.
- End-to-end validation of a feature runs on the private pack `payangar-studio/test-pack` (usual clone `~/dev/minecraft/test-pack`, Modrinth instance "Test Pack"). Its Modrinth project `sqs4OHOo` (`modpack-cli-test-pack`, a draft never submitted for review) is the publication target of tests.

## Release

Pushing a `v*` tag builds every binary of the workspace for `x86_64-unknown-linux-musl` and publishes a GitHub Release with `<binary>-<version>-x86_64-unknown-linux-musl.tar.gz` and its `.sha256` for each. A tag with a suffix (`v0.1.0-rc1`) is published as a prerelease.

Every `uses:` of `.github/` is pinned by full commit SHA with its version in a comment; `scripts/check-workflow-pins.sh` enforces it in CI.

## Harness

- After a finding that could recur beyond the current work (a review, a validation, a human report), ask what in the harness would have prevented it: a check first; otherwise a rule in the closest AGENTS.md; otherwise nothing.
- When a change removes the cause behind such a rule (a check now covers it, the code changed), update or remove the rule.
