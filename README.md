<div align="center">

# mc-cli

[![CI](https://github.com/payangar-studio/mc-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/payangar-studio/mc-cli/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/payangar-studio/mc-cli?include_prereleases)](https://github.com/payangar-studio/mc-cli/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Command-line tools to create, maintain and publish Minecraft modpacks and mods.

[Install](#install) • [Getting started](#getting-started) • [`modpack`](#modpack) • [`mod`](#mod) • [Publication](#publication) • [Development](#development)

</div>

mc-cli ships two binaries, built from one Rust workspace:

- **`modpack`** creates a modpack repository, prepares a clone on a machine, feeds the pack's Modrinth App instance, checks its variants and builds the `.mrpack` and CurseForge packs.
- **`mod`** creates a mod repository (multi-loader Gradle build, tests, design branch), runs the mod in game with other mods, checks it against a dedicated server, and ports it to new Minecraft versions.

Each binary owns a **socle** in the repositories it creates: the files it lays down and keeps up to date (build, CI workflows, git hooks, agent skills, `AGENTS.md` blocks). `modpack update` or `mod update` brings a repository's socle to the CLI's version. Packs and mods are published from CI, through reusable workflows of this repository.

Every command also runs without a terminal (AI agent, git hook, CI): it asks nothing then, and a missing value is an error that names the argument to pass.

> [!NOTE]
> The behaviour of both CLIs is specified in the design documents of `payangar-studio/environment`, branch `design`, under `docs/design/` (`00-cadrage.md`, `01-socle-modpack.md`, `02-socle-mod.md`).

## Install

Each [release](https://github.com/payangar-studio/mc-cli/releases) carries a static Linux build of each binary, `<binary>-<version>-x86_64-unknown-linux-musl.tar.gz`, with its `.sha256`. On Linux x86_64:

```sh
version=0.8.0
for bin in modpack mod; do
  archive="$bin-$version-x86_64-unknown-linux-musl.tar.gz"
  curl -LO "https://github.com/payangar-studio/mc-cli/releases/download/v$version/$archive"
  curl -LO "https://github.com/payangar-studio/mc-cli/releases/download/v$version/$archive.sha256"
  sha256sum -c "$archive.sha256" && tar -xzf "$archive" -C ~/.local/bin
done
```

From source, with a Rust toolchain:

```sh
cargo install --git https://github.com/payangar-studio/mc-cli --locked modpack
cargo install --git https://github.com/payangar-studio/mc-cli --locked mc-mod   # installs `mod`
```

On a machine set up with the studio's chezmoi catalogue, the catalogue installs them.

What else each one needs:

| Binary | Needs |
|---|---|
| `modpack` | The [Modrinth App](https://modrinth.com/app), where `modpack setup` registers the pack's instance |
| `mod` | A JDK 25; Xvfb (`xvfb-run`) for the client tests; the [GitHub CLI](https://cli.github.com/) signed in, or `GH_TOKEN`, for `mod setup` |

## Getting started

### A modpack

```sh
modpack init my-pack          # asks for the name, Minecraft version, loader...
cd my-pack
modpack add sodium            # Modrinth or CurseForge slug, id or URL
modpack setup                 # git hooks, jars, Modrinth App instance
git add . && git commit -m "Create the pack"
```

A teammate who clones the pack runs `modpack setup` once. The git hooks then keep the manifest and the instance in step with each commit, merge and checkout.

### A mod

```sh
mod init my-mod               # asks for the id, name, side, loaders, Minecraft version...
cd my-mod
./gradlew build
mod run --with sodium         # the dev client, with Sodium from Modrinth
```

Once the repository is on GitHub and its Minecraft branch is pushed, `mod setup` protects the release tags, sets the vault variables, makes that branch the default one and publishes the design pages with GitHub Pages.

## `modpack`

| Command | What it does |
|---|---|
| `modpack init [dir] --name --mc-version --loader --loader-version --author` | Creates a modpack repository: socle, layout, default variant. Asks for each value not given. `--author` defaults to `author` under `[identity]` in the [machine configuration](#machine-configuration); when asked for, it can be saved there. |
| `modpack setup [--no-launch]` | Prepares a clone on this machine: enables the git hooks, downloads the jars, registers the instance in the Modrinth App (one click to confirm the import), loads the default variant. Safe to run again. |
| `modpack update [--force]` | Brings the repository's socle to this CLI's version. Stops on socle files edited by hand, unless `--force`. |
| `modpack add <mod> [--version] [--variant]... [--with-deps] [--url] [--force]` | Adds a mod or a resource pack and updates the manifest, in every variant or only in those named (see below). |
| `modpack remove <key>` | Removes a mod from the repository and from the variants that list it. The only way a mod leaves the pack. |
| `modpack manifest [--check]` | Updates `modpack.lock.toml` from the pack's files, then checks `variants.toml` against it. `--check` fails when the manifest is stale. Run by the `pre-commit` hook. |
| `modpack approve <key>`, `modpack approve --check` | Approves a local mod or resource pack for publication. `--check` fails while a local entry is not approved. |
| `modpack install [--force]` | Downloads the missing files, removes the ones it downloaded that the manifest no longer lists, then reloads the variant loaded in the instance. Run by the `post-merge` and `post-checkout` hooks. |
| `modpack variant list` | Lists the variants and marks the loaded one. |
| `modpack variant load <variant> [--force]` | Loads a variant into the instance (see below). |
| `modpack status [--json]` | Loaded variant, drift, files unknown to the manifest, state of the manifest. The JSON format is in [`docs/status.md`](docs/status.md). |
| `modpack test [variant] [--condition] [--json]` | Checks the conditions of a variant, or of all variants. |
| `modpack pack [variant] [--platform] [--out] [--pack-version] [--json]` | Builds the `.mrpack` and the CurseForge zip of a variant, or of all variants, into `dist/`. |
| `modpack page push [--platform]` | Pushes `page/` to the platforms that have an API (Modrinth, slug included once the project exists) and names the skill to run for the others (CurseForge). Never in CI. |
| `modpack auth [secret] [--status] [--remove <secret>]` | Stores the platform secrets in the system keyring, with masked input. See [Secrets](#secrets). |

The variants of a pack and their conditions are written in `variants.toml`, described in [`docs/variants.md`](docs/variants.md).

### Adding mods

`modpack add` takes a mod or a resource pack from:

- Modrinth or CurseForge: slug, id or URL;
- a GitHub release: `github:<owner>/<repo>[@<tag>[/<file>]]`;
- a local file: a `.jar` for a mod, a `.zip` for a resource pack.

`--variant` repeats, `default` names the default variant, and `--variant default` alone keeps the mod in the default variant only. The command asks for the version when several fit. On a mod already in the pack, `--with-deps` adds only its missing dependencies.

Mods and resource packs no platform serves are tracked by git in `mods-local/` and `resourcepacks-local/`, and approved before publication: see [`docs/local-mods.md`](docs/local-mods.md). An entry from a platform whose slug is the key of a local entry of the same type replaces it, and its file leaves git; it stops when that file is not committed, unless `--force`.

### The manifest

`modpack manifest` reads `mods/`, `resourcepacks/`, `mods-local/` and `resourcepacks-local/`:

- it adds the new files, typed by their directory; a file of a local directory becomes a local entry, without asking any platform;
- it rebuilds the entries whose file changed;
- it asks the platforms again for the ids an entry lacks (CurseForge ids missing because no Core API key was set, say).

A missing file keeps its entry, with a warning: not installed here is not removed. A file `modpack install` downloaded that the manifest no longer lists is never added back.

### Loading a variant

`modpack variant load` writes into the instance the variant's jars, its resource packs (in `resourcepacks/`, not enabled: the player turns them on in game), the base config and the variant's overrides, and removes the config files written for the previous variant. It refuses while Minecraft runs. Unless `--force`, it stops on files changed in the instance, and on config files of the instance it did not write that the variant would change.

## `mod`

| Command | What it does |
|---|---|
| `mod init [dir] --id --name --package --side --loaders --license --mc-version [--force]` | Creates a mod repository: socle, Gradle build, entrypoints, and a local `design` branch. Asks for each value not given. `--package` defaults to `<package>.<id without _>`, `package` being under `[identity]` in the [machine configuration](#machine-configuration). |
| `mod update [--force]` | Brings the repository's socle to this CLI's version. Stops on socle files edited by hand, unless `--force`. |
| `mod setup [--no-github] [--vault-url] [--identity-id]` | Configures the mod's GitHub repository: ruleset of the release tags, vault variables, `CODEOWNERS`, default branch, and the `design` branch pushed and served by GitHub Pages. |
| `mod port <minecraft>` | Ports the mod to another Minecraft version: writes the versions pinned for it, then creates and switches to a branch named after it. Commits nothing. |
| `mod run [--profile] [--with]... [--pack]... [--loader] [--server] [--prepare-only]` | Runs the mod in the dev client, alone, with extra mods and resource packs, or with a versioned test profile; `--server` adds a dedicated dev server the client joins. See [`docs/mod-run.md`](docs/mod-run.md). |
| `mod test [--loader]... [--case]... [--ticks]` | Runs the side matrix: a dedicated server and a client with different mod sets, for each loader of the mod. Linux and macOS only; about 20 minutes, so the mod's CI does not run it. See [`docs/mod-tests.md`](docs/mod-tests.md#side-matrix). |

Each Minecraft version of a mod lives on its own branch, named after the version. The `design` branch is an orphan branch holding the mod's design, as Markdown for agents and as a wiki of pages for people; the `mod-wiki-design` skill builds the pages.

The tests `mod init` sets up (unit, integration, server and client gametests, screenshot comparisons) run through Gradle, the same locally and in CI: see [`docs/mod-tests.md`](docs/mod-tests.md).

## Shared behaviour

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Internal failure |
| `2` | Usage error: missing or wrong argument, not in a repository of the right type |
| `3` | Refused, overridable with `--force`; or a check that did not pass (a failed `modpack test` condition, a platform `modpack pack` cannot build for) |

Errors say what happened and what to do. `-v` and `-q` set how much is logged on stderr; command output goes to stdout.

### Machine configuration

`<user config dir>/minecraft/config.toml`, shared by every binary, holds the defaults of `init` under `[identity]`: `author` for `modpack init`, `package` for `mod init`.

### Secrets

The CLIs read each secret from its environment variable first, then from the system keyring (Secret Service on Linux, Credential Manager on Windows), where `modpack auth` stores it:

| Secret | Variable | Needed by |
|---|---|---|
| Modrinth personal access token | `MC_MODRINTH_TOKEN` | `modpack page push` |
| CurseForge upload token | `MC_CURSEFORGE_UPLOAD_TOKEN` | Publication, in CI only |
| CurseForge Core API key | `MC_CURSEFORGE_API_KEY` | Identifying and downloading the mods hosted on CurseForge |

Looking a jar up on Modrinth needs no secret.

> [!IMPORTANT]
> Run `modpack auth` in your own terminal: values are never passed as arguments nor shown.

Keyring entries live under the service `minecraft`, one account per secret (`modrinth-token`, `curseforge-upload-token`, `curseforge-api-key`), so that every binary reads the same ones. An entry an older release stored under the service `modpack-cli` is moved to `minecraft` the first time it is read, with a warning on stderr. The variables of older releases (`MODPACK_MODRINTH_TOKEN`, `MODPACK_CURSEFORGE_UPLOAD_TOKEN`, `MODPACK_CURSEFORGE_API_KEY`) are no longer read: one still set while its `MC_*` variable is not gets a warning, and a missing secret's error names it.

## Publication

Packs and mods are published from CI, never from a machine. The socle lays down a workflow in each repository that calls a reusable workflow of this repository, pinned to the release that wrote the socle; `update` moves the pin. Both read their secrets from the vault (Infisical) by OIDC, as the identity `ci-minecraft`: project `minecraft`, environment `prod`, folder `/app`, under the names of the variables the CLIs read. Nothing goes into GitHub secrets.

Each repository needs two repository variables (Settings, Secrets and variables, Actions, Variables), neither of them a secret: `INFISICAL_URL`, the vault's URL, and `INFISICAL_IDENTITY_ID`, the id of `ci-minecraft`. `mod setup` sets them on a mod; on a pack, set them by hand (an organization variable does not reach a private repository on GitHub Free).

### A modpack

`.github/workflows/modpack-publish.yml` calls [`publish-modpack.yml`](.github/workflows/publish-modpack.yml). Pushing a `v*` tag on `main` of the pack:

1. checks that the tagged commit is on `origin/main`, and stops otherwise (a guard against a tag on a work branch, not a security boundary);
2. installs `modpack` from this repository's release;
3. stops when a local mod is not approved (`modpack approve --check`), naming each one and the command to run; a dry run only warns;
4. runs `modpack pack` for each published variant, from the manifest and the files of `mods-local/` and `resourcepacks-local/`: nothing is downloaded;
5. reads the secrets from the vault;
6. creates a version on Modrinth and uploads a file to CurseForge for each pack. A platform that fails does not stop the other.

The pack also needs its project ids in `page/page.toml`: `[modrinth] project_id`, written by `modpack page push`, and `[curseforge] project_id`, written by the `mc-curseforge-page` skill. A platform whose project id is empty is skipped, with a notice in the run summary. The optional repository variable `MODPACK_PUBLISH_VARIANTS` lists the variants to publish, separated by spaces (all of them when unset).

What gets published:

- version `1.2.0` from tag `v1.2.0`. A suffix makes a prerelease: `-alpha…` is an alpha, any other suffix a beta;
- the default variant as the version itself, every other variant as `<version>+<variant>` on Modrinth and `<name> <version> (<variant>)` on CurseForge;
- the changelog is `changelogs/<version>.md` when it exists, else the tag's message;
- on each platform, the other variants first and the default variant last, so that the newest version is always the default variant's.

> [!TIP]
> Running the workflow by hand (Actions, Publish, Run workflow) from the tag publishes again, to one platform only if needed. With `dry-run`, it builds the packs and keeps them as a workflow artifact without reading the vault or publishing; a dry run also works from a branch, as version `0.0.0-dev`.

When the default variant fails to publish, the run fails and its summary says so: the other variants may already be published, and the default variant is published alone by running the workflow by hand from the tag with variants `default`.

A Modrinth project deleted on Modrinth is recovered by emptying `[modrinth] project_id` in `page/page.toml`, then running `modpack page push`, which creates a new project and records its id. Commit it: the next tag publishes to the new project, while a tag made before it still reads the old id from its own `page/page.toml`.

### A mod

`.github/workflows/mod-publish.yml` calls [`publish-mod.yml`](.github/workflows/publish-mod.yml), which publishes the mod's jars to Modrinth and CurseForge, one version per loader, numbered `<version>+<minecraft>-<loader>`.

- The tag is `v<version>` (`v1.2.0`), or `v<version>+<minecraft>` (`v1.2.0+26.3`) when the same version ships on several Minecraft branches.
- Its commit must be on the branch named after the mod's Minecraft version, and hold `changelogs/<version>.md`.
- Running the workflow by hand from a tag with `dry-run` checks the tag and shows what would be published.

> [!WARNING]
> Push at most three tags at a time: GitHub starts no workflow for a push of more than three tags.

## Development

The repository is a Cargo workspace:

| Crate | Builds |
|---|---|
| [`crates/modpack/`](crates/modpack) | The `modpack` binary |
| [`crates/mod/`](crates/mod) | The `mod` binary (package `mc-mod`), and the Gradle build, harness and shared library of the mods it creates |
| [`crates/mc-core/`](crates/mc-core) | What the binaries share: errors, prompts, secrets, platforms, socle mechanics, mod resolution, page model, dedicated server |

```sh
cargo build
cargo test <filter>
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

[`AGENTS.md`](AGENTS.md) describes the layout, the conventions, how the tests are organized (some need the network, a JDK 25 or Xvfb, and are `#[ignore]`) and how releases are made. Pushing a `v*` tag publishes a release with the static Linux build of every binary.
