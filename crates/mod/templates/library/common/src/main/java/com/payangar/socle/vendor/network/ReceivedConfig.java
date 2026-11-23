package com.payangar.socle.vendor.network;

import com.google.gson.JsonElement;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.payangar.socle.vendor.config.ConfigSchema;
import com.payangar.socle.vendor.config.ConfigValues;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.Setting;
import java.util.List;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The server settings as the client received them from the server it plays on. Until they arrive,
 * and on a server without the mod (a mod whose other side is optional), they are the defaults.
 * Read-only: no client changes server settings over the network. Used on the client thread.
 */
public final class ReceivedConfig {
    private final ConfigSchema schema;
    private final Logger logger;
    private volatile ConfigValues values;
    private volatile boolean synced;

    ReceivedConfig(ConfigSchema schema) {
        this.schema = schema;
        this.logger = LoggerFactory.getLogger(schema.modId());
        this.values = new ConfigValues(schema, Scope.SERVER);
    }

    /** Whether the server sent its settings since the client joined it. */
    public boolean synced() {
        return synced;
    }

    public <T> T get(Setting<T> setting) {
        return values.get(setting);
    }

    /** A copy of the settings as received now, to show them. */
    public ConfigValues values() {
        return values.copy();
    }

    /**
     * Takes the settings the server sent. A setting it does not send keeps its default; a value
     * the client cannot read (another version of the mod) takes the default and is logged.
     */
    public void receive(ConfigPayloads.Sync sync) {
        ConfigValues received = new ConfigValues(schema, Scope.SERVER);
        List<String> problems;
        try {
            JsonElement json = JsonParser.parseString(sync.values());
            problems = json.isJsonObject() ? received.read(json.getAsJsonObject()) : List.of("not a JSON object");
        } catch (JsonParseException e) {
            problems = List.of("not a JSON object (" + e.getMessage() + ")");
        }
        if (!problems.isEmpty()) {
            logger.warn("Server settings of {} received from the server with problems (a value the client cannot"
                    + " read keeps its default): {}", schema.modId(), String.join("; ", problems));
        }
        values = received;
        synced = true;
    }

    /** Back to the defaults, when the client leaves the server. */
    public void reset() {
        values = new ConfigValues(schema, Scope.SERVER);
        synced = false;
    }
}
