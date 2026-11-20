# Tests of a mod

What `mod init` sets up in a mod repository, and the commands that run it. They are the same locally and in CI (`.github/workflows/mod-tests.yml`). The client runs need Xvfb (`xvfb-run` and `Xvfb`). Without it, the client opens a window and renders on the GPU, so a screenshot comparison can fail on an unchanged mod: the references are software renders (see below).

| Command | Runs |
|---|---|
| `./gradlew check` | Unit tests (`src/test`, JUnit), NeoForge integration tests (`neoforge/src/test`, game loaded), server gametests of each loader, NeoForge's dedicated server self-test |
| `./gradlew clientCheck` | Fabric client gametests on the dev client and on a production client, NeoForge's client self-test |
| `./gradlew :fabric:runClientGameTest` | Fabric client gametests only |
| `./gradlew :fabric:promoteScreenshots -Pscreenshots=<dir>` | Makes the renders of a CI run the references |

A test profile with `ci = true` adds a job to the workflow: Fabric's gametests with that profile's mods, screenshot comparisons skipped (see [`mod run`](mod-run.md#ci-profiles)).

## Where tests go

| Layer | Fabric | NeoForge |
|---|---|---|
| Unit | `common/src/test/java` (`GameBootstrap.run()` loads the registries); `fabric/src/test/java` runs inside Fabric Loader, and holds all of them in a Fabric-only mod | `common/src/test/java`; a NeoForge-only mod has them in `neoforge/src/test/java` |
| Integration | none | `neoforge/src/test/java`, `@ExtendWith(EphemeralTestServerProvider.class)` for a server |
| Server gametests | `fabric/src/gametest`: `@GameTest` methods of a class listed under `fabric-gametest` in its `fabric.mod.json` | `neoforge/src/gametest`: test functions registered through `RegisterGameTestsEvent` |
| Client gametests | `fabric/src/gametest`: a `FabricClientGameTest` listed under `fabric-client-gametest` | none |

The gametests of each loader belong to a test mod, `<id>_gametest`, loaded only by the test runs.

## Guards

- A test task that ran no test fails, and says where to write one.
- A failing gametest fails its run.
- A screenshot comparison fails on a changed render, and on a missing reference: a run never writes a reference.

## Screenshot references

References live in `fabric/src/gametest/resources/templates/`. They are renders of CI (Mesa's software renderer, OpenGL pinned in `options.txt`), never of a local GPU. To add or update one:

1. Push; the `client` job uploads the renders as the `screenshots` artifact.
2. `gh run download <run id> -n screenshots -D <dir>`
3. `./gradlew :fabric:promoteScreenshots -Pscreenshots=<dir>`, then commit the reference.

## Side matrix

`mod test` checks the mod against the other side, which no loader can do in one process: a dedicated server with one set of mods and a client with another, joining it directly. The cases follow `side` in `mod.toml` and run on each loader of the mod:

| Side | Cases |
|---|---|
| `client` | `server-holds-jar`: a dedicated server holding the jar starts and keeps running; `modded-client-joins-unmodded-server`: the client with the mod joins a server without it and plays |
| `server` | `server-starts`: the dedicated server starts with the mod; `vanilla-client-joins`: a vanilla client joins and plays |
| `both-required` | `server-starts`; `modded-client-joins`: a client with the mod joins a server with the mod; `vanilla-client-refused`: a vanilla client is refused with the message of the library's side check |
| `both-optional` | `server-starts`, `modded-client-joins`, `vanilla-client-joins`, `modded-client-joins-unmodded-server` |

- The mod set of a side with the mod is its jar (`./gradlew :<loader>:jar`, run by `mod test`) and its required dependencies; only Fabric API is fetched so far, another required dependency fails the run. A side without the mod has its loader only.
- The client is launched by HeadlessMC (pinned version, downloaded into the machine cache), without a display and with an offline account: the vanilla client, Fabric's, or NeoForge's (installed by its installer).
- The verdict comes from the server's log: the player joined and is still there after `--ticks` (200 by default), or was refused with the expected message. A case with the mod also checks that each side with the mod loaded it.
- Each case writes the server's log, the client's game directory and HeadlessMC's output under `runs/sides/<loader>/<case>/`.
- What the mod's feature does in game is not part of the matrix: client gametests cover it, on a server that has the same mods.
- Linux and macOS only: the client is stopped through its process group.
