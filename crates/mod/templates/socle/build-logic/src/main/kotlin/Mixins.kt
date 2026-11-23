// Managed by mc-cli: `mod update` rewrites this file.
//
// The mixin configurations the loader metadata declares. The mod writes them,
// named as in the loaders' templates (Fabric's example mod, NeoForge's MDK,
// MultiLoader-Template), in the resources of the subproject that holds their
// mixins, `common` included:
// - `<id>.mixins.json`, in the main resources: declared by every loader whose
//   jar ships it;
// - `<id>.fabric.mixins.json`, `<id>.neoforge.mixins.json`, in the main
//   resources: by that loader;
// - `<id>.client.mixins.json`, in the client resources: by Fabric, for the
//   client only. NeoForge has no client-only configuration: there, client
//   mixins go under `client` in another configuration.
// One of these names elsewhere would ship undeclared: it fails the build.

import java.io.File
import org.gradle.api.GradleException
import org.gradle.api.Project

/** A mixin configuration, applied on the client only when `client`. */
data class MixinConfig(val name: String, val client: Boolean)

/** The mixin configurations of `loader` found in its subproject and in `common`. */
fun Project.mixinConfigs(mod: ModToml, loader: String): List<MixinConfig> {
    val subprojects = listOfNotNull(projectDir, rootProject.findProject(":common")?.projectDir)
    fun find(sourceSet: String, name: String) =
        subprojects.map { it.resolve("src/$sourceSet/resources/$name") }.firstOrNull { it.isFile }
    fun fail(file: File, problem: String): Nothing =
        throw GradleException("${file.relativeTo(rootDir).invariantSeparatorsPath}: $problem")

    val names = listOf("${mod.id}.mixins.json", "${mod.id}.$loader.mixins.json")
    names.forEach { name ->
        find("client", name)?.let {
            fail(it, "the build declares $name from src/main/resources only. Move it there; client mixins go under its `client` key.")
        }
    }
    val configs = names.filter { find("main", it) != null }.map { MixinConfig(it, client = false) }

    val client = "${mod.id}.client.mixins.json"
    val clientFile = find("client", client)
    val misplaced = find("main", client)
    if (loader != "fabric") {
        (clientFile ?: misplaced)?.let {
            fail(it, "NeoForge has no client-only mixin configuration. List these mixins under `client` in ${mod.id}.mixins.json instead.")
        }
        return configs
    }
    misplaced?.let { fail(it, "the build declares $client from src/client/resources only. Move it there.") }
    return if (clientFile == null) configs else configs + MixinConfig(client, client = true)
}
