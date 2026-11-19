package com.payangar.socle.vendor.network;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.payangar.socle.vendor.config.ConfigSchema;
import com.payangar.socle.vendor.config.ConfigValues;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.Setting;
import java.util.List;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The server settings as the client received them from the server it plays on. Until they arrive,
 * and on a server without the mod (a mod whose other side is optional), they are the defaults.
 * Read-only: a change goes to the server through {@link #request}, and comes back with the next
 * settings it sends. Used on the client thread.
 */
public final class ReceivedConfig {
    private final ConfigSchema schema;
    private final ConfigPayloads payloads;
    private final Logger logger;
    private volatile ConfigValues values;
    private volatile boolean synced;
    private volatile BooleanSupplier serverListens = () -> false;
    private volatile Consumer<CustomPacketPayload> toServer = payload -> {};

    ReceivedConfig(ConfigSchema schema, ConfigPayloads payloads) {
        this.schema = schema;
        this.payloads = payloads;
        this.logger = LoggerFactory.getLogger(schema.modId());
        this.values = new ConfigValues(schema, Scope.SERVER);
    }

    /**
     * Called by the loader's client code: whether the server receives changes, and how to send them
     * there.
     */
    public void connect(BooleanSupplier serverListens, Consumer<CustomPacketPayload> toServer) {
        this.serverListens = serverListens;
        this.toServer = toServer;
    }

    /** Whether the server sent its settings since the client joined it. */
    public boolean synced() {
        return synced;
    }

    public <T> T get(Setting<T> setting) {
        return values.get(setting);
    }

    /** A copy of the settings to edit, then to pass to {@link #request}. */
    public ConfigValues edit() {
        return values.copy();
    }

    /**
     * Asks the server to set the settings that differ between {@code edited} and the received ones.
     * The server applies the change only for an operator ({@link ServerConfig#canEdit}), then sends
     * the settings to every player.
     *
     * @return whether the change was sent: not when nothing differs, nor before the server sent its
     *     settings, nor to a server without the mod
     */
    public boolean request(ConfigValues edited) {
        JsonObject before = values.settingsJson();
        JsonObject after = edited.settingsJson();
        JsonObject changes = new JsonObject();
        for (String key : after.keySet()) {
            if (!after.get(key).equals(before.get(key))) {
                changes.add(key, after.get(key));
            }
        }
        if (changes.isEmpty() || !synced || !serverListens.getAsBoolean()) {
            return false;
        }
        toServer.accept(payloads.change(changes.toString()));
        return true;
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
            logger.warn("Server settings of {} received from the server with problems, defaults used for them: {}",
                    schema.modId(), String.join("; ", problems));
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
