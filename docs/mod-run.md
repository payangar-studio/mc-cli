# `mod run`

Runs the mod in the game through the repository's Gradle wrapper (the loader's dev client), alone or with extra mods and resource packs from Modrinth. Everything it writes goes under `runs/`, which the socle's `.gitignore` block ignores.

```sh
mod run                                     # the mod alone
mod run --with sodium --with lithium@mc26.3-0.26.3-fabric --pack fresh-animations
mod run --profile sodium                    # test-profiles/sodium.toml
mod run --profile sodium --server           # plus a dev server the client joins
mod run --loader neoforge                   # another of the mod's loaders
```

- **One-shot** (`--with`, `--pack`, repeatable): the run directory is `runs/.one-shot/`, emptied at the start of each one-shot run.
- **Profile** (`--profile <name>`): the run directory is `runs/<name>/` and is kept between runs, so the world and the options the player changed stay. Its `mods/` and `resourcepacks/` follow the profile: files no longer listed are removed.
- `<slug>@<version>` pins a version, by its Modrinth version number or id; without it, the latest release compatible with the branch's Minecraft and the loader is taken (a beta when there is no release). Required dependencies are not added: list them too.
- The game directory is the run directory: `logs/latest.log`, `screenshots/`, `crash-reports/` are there.

## Test profiles

`test-profiles/<name>.toml`, versioned. Every key is optional.

```toml
loader = "fabric"        # the loader it runs on; the mod's first loader when absent
mods = ["sodium", "lithium@mc26.3-0.26.3-fabric"]
packs = ["fresh-animations"]   # enabled in this order, the last one on top

[options]                # written to options.txt as <key>:<value>
renderDistance = 8
lang = "fr_fr"
```

The resource packs are enabled in `options.txt` (`resourcePacks`), after the packs the game itself lists. A value of `[options]` is written as it is: a string option the game stores quoted is written quoted (`soundDevice = '""'`).

## Dev server

`--server` starts the loader's dev server in `<run dir>/server/` and waits for its `Done`, then starts the client with `--quickPlayMultiplayer`. The server listens on this machine only (`server-ip=127.0.0.1`), in offline mode, without whitelist, on port 25565 (another free port when it is taken). RCON is on, port 25575 by default, with a password generated once and kept in `<run dir>/server/server.properties`. The server's Gradle output goes to `<run dir>/server/gradle.log`, its log to `<run dir>/server/logs/latest.log`. Closing the client stops the server. Mods Modrinth marks client-only stay out of the server, server-only ones out of the client.
