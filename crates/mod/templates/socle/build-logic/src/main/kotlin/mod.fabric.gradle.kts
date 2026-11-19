// Managed by mc-cli: `mod update` rewrites this file.
//
// The Fabric jar, through Loom without remapping. The client code lives in the
// `client` source set. fabric.mod.json is generated from mod.toml.

import net.fabricmc.loom.api.LoomGradleExtensionAPI

plugins {
    id("mod.java")
    id("net.fabricmc.fabric-loom")
}

val mod = ModToml.read(rootDir.resolve("mod.toml"))
val sourceSets = the<SourceSetContainer>()
val pinned = Pinned.read(rootDir.resolve(".mod/socle.toml"))

extensions.configure<BasePluginExtension> {
    archivesName.set("${mod.id}-fabric")
}

extensions.configure<LoomGradleExtensionAPI> {
    splitEnvironmentSourceSets()
    mods.register(mod.id) {
        sourceSet(sourceSets["main"])
        sourceSet(sourceSets["client"])
        // common's classes, on the runtime classpath below, belong to the mod
        // in dev runs too.
        if (hasCommon) {
            sourceSet("main", project(":common"))
            sourceSet("client", project(":common"))
        }
    }
}

dependencies {
    "minecraft"("com.mojang:minecraft:${pinned.minecraft}")
    "implementation"("net.fabricmc:fabric-loader:${pinned.fabricLoader}")
    if (mod.dependenciesOf("fabric").any { it.id == "fabric-api" }) {
        "implementation"("net.fabricmc.fabric-api:fabric-api:${pinned.fabricApi}")
    }
}

if (hasCommon) {
    val mainClasses = fromCommon("commonMainClasses")
    val clientClasses = fromCommon("commonClientClasses")
    sourceSets["main"].compileClasspath += mainClasses
    sourceSets["main"].runtimeClasspath += mainClasses
    sourceSets["client"].compileClasspath += mainClasses + clientClasses
    sourceSets["client"].runtimeClasspath += mainClasses + clientClasses
    tasks.named<Jar>("jar") {
        from(mainClasses, clientClasses)
    }
    tasks.named<ProcessResources>("processResources") {
        from(fromCommon("commonMainResources"))
    }
    tasks.named<ProcessResources>("processClientResources") {
        from(fromCommon("commonClientResources"))
    }
}

val generateModMetadata = tasks.register<Copy>("generateModMetadata") {
    val values = ModMetadata.fabric(mod, pinned, project.version.toString())
    inputs.properties(values)
    from(rootDir.resolve("build-logic/templates/fabric.mod.json"))
    expand(values)
    into(layout.buildDirectory.dir("generated/modMetadata"))
}
sourceSets["main"].resources.srcDir(generateModMetadata)
