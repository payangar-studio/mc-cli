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

import groovy.json.JsonSlurper
import java.io.File
import java.util.zip.ZipFile
import org.gradle.api.Project
import org.gradle.api.Task

/** Game directory of the runs, or null for the loader's default. */
val Project.modRunDir: File?
    get() = (findProperty("mod.runDir") as String?)?.takeIf { it.isNotBlank() }?.let(::File)

/** Program arguments of the client run: join the server of `mod run --server`. */
val Project.joinServerArgs: List<String>
    get() = (findProperty("mod.joinServer") as String?)?.takeIf { it.isNotBlank() }
        ?.let { listOf("--quickPlayMultiplayer", it) }
        ?: emptyList()

/**
 * Before the client run in the game directory `dir` of `mod run`: gives the
 * options.txt `mod run` wrote (a profile's options, the resource packs) the
 * data version of the Minecraft jar on `classpath` when it has none, so that
 * the game keeps them as they are written.
 */
fun Task.stampOptionsVersion(dir: File, classpath: Iterable<File>) {
    doFirst {
        val options = dir.resolve("options.txt")
        if (options.isFile) {
            val text = options.readText()
            if (text.lineSequence().none { it.startsWith("version:") }) {
                options.writeText("version:${dataVersion(classpath)}\n$text")
            }
        }
    }
}

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

/** Whether a jar of `dir` is the Fabric mod `id`, by its fabric.mod.json. */
fun holdsFabricMod(dir: File, id: String): Boolean =
    dir.listFiles { f -> f.isFile && f.extension == "jar" }.orEmpty().any { jar ->
        ZipFile(jar).use { zip ->
            zip.getEntry("fabric.mod.json")?.let { entry ->
                val json = zip.getInputStream(entry).use { JsonSlurper().parse(it, "UTF-8") }
                (json as? Map<*, *>)?.get("id") == id
            } ?: false
        }
    }
