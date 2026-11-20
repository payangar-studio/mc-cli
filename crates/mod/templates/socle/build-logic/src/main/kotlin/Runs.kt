// Managed by mc-cli: `mod update` rewrites this file.
//
// What `mod run` passes to the build, as Gradle properties: the game directory
// of the run (`mod.runDir`, absolute: `runs/<profile>/`) and, with
// `mod run --server`, the server the client joins (`mod.joinServer`,
// `host:port`). Without them, the runs keep the loaders' defaults.
//
// The mod's CI passes `mod.testProfile`, the run directory of a test profile
// that `mod run --prepare-only --server` filled: the Fabric gametests then
// run with the profile's mods.

import java.io.File
import org.gradle.api.Project

/** Game directory of the runs, or null for the loader's default. */
val Project.modRunDir: File?
    get() = (findProperty("mod.runDir") as String?)?.takeIf { it.isNotBlank() }?.let(::File)

/** Program arguments of the client run: join the server of `mod run --server`. */
val Project.joinServerArgs: List<String>
    get() = (findProperty("mod.joinServer") as String?)?.takeIf { it.isNotBlank() }
        ?.let { listOf("--quickPlayMultiplayer", it) }
        ?: emptyList()

/**
 * Run directory of the test profile the gametests run with
 * (`mod.testProfile`, relative to the repository root or absolute), or null.
 */
val Project.testProfileDir: File?
    get() = (findProperty("mod.testProfile") as String?)?.takeIf { it.isNotBlank() }?.let { rootDir.resolve(it) }

/**
 * System properties of a Fabric gametest run under the test profile at
 * `dir`: Fabric Loader adds the jars of `<dir>/<mods>` (the loader empties
 * the client gametest run directory, so they cannot live there), and
 * `mod.testProfile`, the profile's name, tells the screenshot comparisons to
 * skip: other mods change the render.
 */
fun testProfileProperties(dir: File, mods: String): Map<String, String> = mapOf(
    "fabric.addMods" to dir.resolve(mods).absolutePath,
    "mod.testProfile" to dir.name,
)
