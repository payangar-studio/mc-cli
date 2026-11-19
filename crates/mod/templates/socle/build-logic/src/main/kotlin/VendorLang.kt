// Managed by mc-cli: `mod update` rewrites this file.
//
// The strings of the shared library (the `vendor` package): `mod update`
// writes them to .mod/vendor/lang/<locale>.json, keys without prefix. The game
// reads one language file per namespace and several mods carry the library,
// so each loader's jar gets them merged into the mod's own language file,
// assets/<modid>/lang/<locale>.json, under `<modid>.vendor.`. A key the mod's
// file defines itself is kept.

import groovy.json.JsonGenerator
import groovy.json.JsonOutput
import groovy.json.JsonSlurper
import java.io.File
import org.gradle.api.GradleException
import org.gradle.api.Project
import org.gradle.kotlin.dsl.named
import org.gradle.language.jvm.tasks.ProcessResources

/** Merges the library's strings into the language files `processResources` writes. */
fun Project.mergeVendorLang(modId: String) {
    val library = rootDir.resolve(".mod/vendor/lang")
    tasks.named<ProcessResources>("processResources") {
        inputs.files(fileTree(library)).withPropertyName("vendorLang")
        inputs.property("vendorLangModId", modId)
        doLast {
            val lang = destinationDir.resolve("assets/$modId/lang")
            library.listFiles { f -> f.isFile && f.extension == "json" }.orEmpty().sorted().forEach { file ->
                val target = lang.resolve(file.name)
                val merged = LinkedHashMap(if (target.isFile) readLang(target) else emptyMap())
                readLang(file).forEach { (key, value) -> merged.putIfAbsent("$modId.vendor.$key", value) }
                target.parentFile.mkdirs()
                val json = JsonGenerator.Options().disableUnicodeEscaping().build().toJson(merged)
                target.writeText(JsonOutput.prettyPrint(json, true) + "\n")
            }
        }
    }
}

/** A language file: a JSON object of strings. */
private fun readLang(file: File): Map<String, String> {
    val parsed = try {
        JsonSlurper().parse(file, "UTF-8")
    } catch (e: Exception) {
        throw GradleException("${file.path} is not valid JSON: ${e.message}", e)
    }
    val map = parsed as? Map<*, *> ?: throw GradleException("${file.path} must hold a JSON object")
    return map.entries.associate { (key, value) ->
        key as String to (value as? String ?: throw GradleException("${file.path}: `$key` must be a string"))
    }
}
