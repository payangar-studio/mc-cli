package com.payangar.socle.vendor.config;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Where the settings of each scope are stored, and the JSON file itself.
 *
 * <ul>
 *   <li>client scope: {@code <config dir>/<modid>.json}, the loader's config directory;
 *   <li>server scope: {@code <world dir>/serverconfig/<modid>.json}, in the folder of the world.
 * </ul>
 */
public final class ConfigFile {
    /** The folder of the server settings inside the world folder. */
    public static final String SERVER_DIR = "serverconfig";

    private static final Gson GSON = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();

    private ConfigFile() {}

    /** The file of the client settings, in the loader's config directory {@code configDir}. */
    public static Path clientPath(Path configDir, String modId) {
        return configDir.resolve(modId + ".json");
    }

    /** The file of the server settings of the world whose folder is {@code worldDir}. */
    public static Path serverPath(Path worldDir, String modId) {
        return worldDir.resolve(SERVER_DIR).resolve(modId + ".json");
    }

    /** The settings of a scope as loaded, and what was wrong with the file. */
    public record Loaded(ConfigValues values, List<String> problems) {}

    /**
     * Loads the settings of {@code scope} from {@code file}.
     *
     * <ul>
     *   <li>A missing file is written with the defaults.
     *   <li>A missing key takes its default. A value out of bounds is clamped, a value of the wrong
     *       type takes the default; both are reported. The file is then written back with the
     *       values in use.
     *   <li>A key no setting of the scope declares is kept as it is.
     *   <li>A file that is not a JSON object is reported and left untouched; the defaults are used.
     * </ul>
     *
     * Each problem is also logged as a warning.
     *
     * @throws IOException when the file cannot be read or written
     */
    public static Loaded load(ConfigSchema schema, Scope scope, Path file) throws IOException {
        ConfigValues values = new ConfigValues(schema, scope);
        List<String> problems = new ArrayList<>();
        if (!Files.exists(file)) {
            save(values, file);
            return new Loaded(values, List.of());
        }
        JsonObject json;
        try {
            JsonElement root = JsonParser.parseString(Files.readString(file, StandardCharsets.UTF_8));
            if (!root.isJsonObject()) {
                throw new JsonParseException("not a JSON object");
            }
            json = root.getAsJsonObject();
        } catch (JsonParseException e) {
            problems.add(file + " is not a JSON object (" + e.getMessage() + "): defaults are used, and the"
                    + " file is left as it is. Fix it or delete it, then restart.");
            report(schema, problems);
            return new Loaded(values, List.copyOf(problems));
        }

        boolean changed = false;
        for (Setting<?> setting : schema.settings(scope)) {
            JsonElement stored = json.get(setting.name());
            if (stored == null) {
                changed = true;
            } else {
                changed |= read(values, setting, stored, file, problems);
            }
        }
        for (Map.Entry<String, JsonElement> entry : json.entrySet()) {
            if (schema.settings(scope).stream().noneMatch(s -> s.name().equals(entry.getKey()))) {
                values.keepUnknown(entry.getKey(), entry.getValue());
            }
        }
        if (changed) {
            save(values, file);
        }
        report(schema, problems);
        return new Loaded(values, List.copyOf(problems));
    }

    /**
     * Writes {@code values} to {@code file}, creating its folder. The file is replaced at once: a
     * crash while writing leaves the previous one.
     */
    public static void save(ConfigValues values, Path file) throws IOException {
        Path dir = file.toAbsolutePath().getParent();
        Files.createDirectories(dir);
        Path temp = Files.createTempFile(dir, file.getFileName().toString(), ".tmp");
        try {
            Files.writeString(temp, GSON.toJson(values.toJson()) + "\n", StandardCharsets.UTF_8);
            try {
                Files.move(temp, file, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
            } catch (AtomicMoveNotSupportedException e) {
                Files.move(temp, file, StandardCopyOption.REPLACE_EXISTING);
            }
        } finally {
            Files.deleteIfExists(temp);
        }
    }

    /** Reads {@code stored} into {@code values}; whether the file must be written back. */
    private static <T> boolean read(
            ConfigValues values, Setting<T> setting, JsonElement stored, Path file, List<String> problems) {
        SettingType.Reading<T> reading = setting.type().read(stored);
        if (reading.value() == null) {
            problems.add(file + ": `" + setting.name() + "`: " + reading.problem() + ", set to the default "
                    + setting.type().write(setting.defaultValue()));
            return true;
        }
        values.set(setting, reading.value());
        if (reading.problem() != null) {
            problems.add(file + ": `" + setting.name() + "`: " + reading.problem());
            return true;
        }
        return false;
    }

    private static void report(ConfigSchema schema, List<String> problems) {
        if (problems.isEmpty()) {
            return;
        }
        Logger logger = LoggerFactory.getLogger(schema.modId());
        problems.forEach(logger::warn);
    }
}
