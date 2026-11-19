// Managed by mc-cli: `mod update` rewrites this file.
//
// What every subproject shares: Java 25, the mod's group and version.

plugins {
    java
}

val mod = ModToml.read(rootDir.resolve("mod.toml"))

group = mod.pkg
if (version == Project.DEFAULT_VERSION) {
    version = "0.0.0"
}

java {
    toolchain.languageVersion.set(JavaLanguageVersion.of(25))
}

tasks.withType<JavaCompile>().configureEach {
    options.encoding = "UTF-8"
}
