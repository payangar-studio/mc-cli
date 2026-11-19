// Managed by mc-cli: `mod update` rewrites this file.
//
// What `mod run` passes to the build, as Gradle properties: the game directory
// of the run (`mod.runDir`, absolute: `runs/<profile>/`) and, with
// `mod run --server`, the server the client joins (`mod.joinServer`,
// `host:port`). Without them, the runs keep the loaders' defaults.

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
