# Tests of a mod

What `mod init` sets up in a mod repository, and the commands that run it. They are the same locally and in CI (`.github/workflows/mod-tests.yml`). The client runs need Xvfb (`xvfb-run` and `Xvfb`); without it, the client opens a window.

| Command | Runs |
|---|---|
| `./gradlew check` | Unit tests (`src/test`, JUnit), NeoForge integration tests (`neoforge/src/test`, game loaded), server gametests of each loader, NeoForge's dedicated server self-test |
| `./gradlew clientCheck` | Fabric client gametests on the dev client and on a production client, NeoForge's client self-test |
| `./gradlew :fabric:runClientGameTest` | Fabric client gametests only |
| `./gradlew :fabric:promoteScreenshots -Pscreenshots=<dir>` | Makes the renders of a CI run the references |

## Where tests go

| Layer | Fabric | NeoForge |
|---|---|---|
| Unit | `common/src/test/java` (`GameBootstrap.run()` loads the registries); `fabric/src/test/java` runs inside Fabric Loader | `common/src/test/java`; a NeoForge-only mod has them in `neoforge/src/test/java` |
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
