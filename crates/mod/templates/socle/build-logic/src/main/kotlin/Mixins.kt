// Managed by mc-cli: `mod update` rewrites this file.
//
// The mixin configurations the loader metadata declares. The mod writes them,
// named as in the loaders' templates (Fabric's example mod, NeoForge's MDK,
// MultiLoader-Template), in the resources of the subproject that holds their
// mixins, `common` included:
// - `<id>.mixins.json`: declared by every loader whose jar ships it;
// - `<id>.fabric.mixins.json`, `<id>.neoforge.mixins.json`: by that loader;
// - `<id>.client.mixins.json`, in the client resources: by Fabric, for the
//   client only. NeoForge has no client-only configuration: there, client
//   mixins go under `client` in another configuration.

import org.gradle.api.GradleException
import org.gradle.api.Project

/** A mixin configuration, applied on the client only when `client`. */
data class MixinConfig(val name: String, val client: Boolean)

/** The mixin configurations of `loader` found in its subproject and in `common`. */
fun Project.mixinConfigs(mod: ModToml, loader: String): List<MixinConfig> {
    val subprojects = listOfNotNull(projectDir, rootProject.findProject(":common")?.projectDir)
    fun found(sourceSet: String, name: String) =
        subprojects.any { it.resolve("src/$sourceSet/resources/$name").isFile }

    val configs = listOf("${mod.id}.mixins.json", "${mod.id}.$loader.mixins.json")
        .filter { found("main", it) }
        .map { MixinConfig(it, client = false) }
    val client = "${mod.id}.client.mixins.json"
    if (!found("client", client)) return configs
    if (loader != "fabric") {
        throw GradleException(
            "common/src/client/resources/$client: NeoForge has no client-only mixin configuration. " +
                "List these mixins under `client` in ${mod.id}.mixins.json instead.",
        )
    }
    return configs + MixinConfig(client, client = true)
}
