// Managed by mc-cli: `mod update` rewrites this file.
//
// The NeoForge jar, through ModDevGradle. NeoForge has no client source set:
// the client code of a NeoForge-only mod lives in the `client` packages, and
// NeoForge hides client classes from a dedicated server in development.
// META-INF/neoforge.mods.toml is generated from mod.toml.

import net.neoforged.moddevgradle.dsl.NeoForgeExtension

plugins {
    id("mod.java")
    id("net.neoforged.moddev")
}

val mod = ModToml.read(rootDir.resolve("mod.toml"))
val sourceSets = the<SourceSetContainer>()
val pinned = Pinned.read(rootDir.resolve(".mod/socle.toml"))

extensions.configure<BasePluginExtension> {
    archivesName.set("${mod.id}-neoforge")
}

extensions.configure<NeoForgeExtension> {
    setVersion(pinned.neoforge)
    mods.register(mod.id) {
        sourceSet(sourceSets["main"])
    }
}

if (hasCommon) {
    val classes = fromCommon("commonMainClasses")
    val clientClasses = fromCommon("commonClientClasses")
    sourceSets["main"].compileClasspath += classes + clientClasses
    sourceSets["main"].runtimeClasspath += classes + clientClasses
    tasks.named<Jar>("jar") {
        from(classes, clientClasses)
    }
    tasks.named<ProcessResources>("processResources") {
        from(fromCommon("commonMainResources"), fromCommon("commonClientResources"))
    }
}

val generateModMetadata = tasks.register<Copy>("generateModMetadata") {
    val values = ModMetadata.neoforge(mod, pinned, project.version.toString())
    inputs.properties(values)
    from(rootDir.resolve("build-logic/templates/neoforge.mods.toml"))
    expand(values)
    into(layout.buildDirectory.dir("generated/modMetadata/META-INF"))
}
sourceSets["main"].resources.srcDir(layout.buildDirectory.dir("generated/modMetadata"))
tasks.named<ProcessResources>("processResources") {
    dependsOn(generateModMetadata)
}
