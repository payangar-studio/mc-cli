package com.payangar.socle.vendor.network;

import com.google.gson.JsonObject;
import com.payangar.socle.vendor.config.ConfigFile;
import com.payangar.socle.vendor.config.ConfigSchema;
import com.payangar.socle.vendor.config.ConfigValues;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.Setting;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Path;
import java.util.Optional;
import java.util.function.BiConsumer;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.storage.LevelResource;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The server settings of the running server, which owns them: loaded from the folder of its world
 * when it starts, sent to its players. A dedicated server's are changed in its world's file; in
 * single player, the integrated server plays that role and the config screen of the player who
 * owns the world changes them ({@link #update}). No client changes them over the network. Used on
 * the server thread.
 */
public final class ServerConfig {
    private final ConfigSchema schema;
    private final ConfigPayloads payloads;
    private final Logger logger;
    private volatile ConfigValues values;
    private volatile Path file;
    private volatile BiConsumer<MinecraftServer, ConfigPayloads.Sync> toPlayers = (server, sync) -> {};

    ServerConfig(ConfigSchema schema, ConfigPayloads payloads) {
        this.schema = schema;
        this.payloads = payloads;
        this.logger = LoggerFactory.getLogger(schema.modId());
    }

    /**
     * Called by the loader's server code: how to send settings to every player whose client has the
     * mod.
     */
    public void connect(BiConsumer<MinecraftServer, ConfigPayloads.Sync> toPlayers) {
        this.toPlayers = toPlayers;
    }

    /**
     * Loads the settings of {@code server}'s world, from {@code <world>/serverconfig/<modid>.json}.
     *
     * @throws UncheckedIOException when the file exists but cannot be read
     */
    public void start(MinecraftServer server) {
        Path path = ConfigFile.serverPath(server.getWorldPath(LevelResource.ROOT), schema.modId());
        try {
            values = ConfigFile.load(schema, Scope.SERVER, path).values();
        } catch (IOException e) {
            throw new UncheckedIOException(schema.modId() + ": cannot read " + path, e);
        }
        file = path;
    }

    /** Forgets the settings of the server that stopped. */
    public void stop() {
        values = null;
        file = null;
    }

    /** A copy of the settings of the running server; empty when none is running. */
    public Optional<ConfigValues> values() {
        return Optional.ofNullable(values).map(ConfigValues::copy);
    }

    /**
     * The value of {@code setting} on the running server.
     *
     * @throws IllegalStateException when no server is running
     */
    public <T> T get(Setting<T> setting) {
        return running().get(setting);
    }

    /**
     * The settings to send to a player; empty, and logged, when they are too long for their payload
     * (a long text written by hand in the world's file).
     */
    public Optional<ConfigPayloads.Sync> sync() {
        String json = running().settingsJson().toString();
        if (json.length() > ConfigPayloads.MAX_SYNC) {
            logger.error("The server settings of {} are not sent to players: {} characters, more than the {} they"
                    + " may hold. Shorten their texts in {}.", schema.modId(), json.length(), ConfigPayloads.MAX_SYNC,
                    file);
            return Optional.empty();
        }
        return Optional.of(payloads.sync(json));
    }

    /**
     * Replaces the settings of {@code server}, the running one, with {@code changed}, saves them to
     * the world's file and sends them to every player. Nothing happens when that server stopped.
     */
    public void update(MinecraftServer server, ConfigValues changed) {
        if (changed.schema() != schema || changed.scope() != Scope.SERVER) {
            throw new IllegalArgumentException("not the server settings of " + schema.modId());
        }
        ConfigValues before = values;
        if (before == null) {
            logger.warn("The server settings of {} were not changed: the server stopped", schema.modId());
            return;
        }
        ConfigValues after = changed.copy();
        values = after;
        JsonObject was = before.settingsJson();
        JsonObject moved = new JsonObject();
        after.settingsJson().entrySet().stream()
                .filter(entry -> !entry.getValue().equals(was.get(entry.getKey())))
                .forEach(entry -> moved.add(entry.getKey(), entry.getValue()));
        logger.info("Server settings of {} changed: {}", schema.modId(), moved);
        try {
            ConfigFile.save(after, file);
        } catch (IOException e) {
            logger.error("The server settings of {} apply until the server stops, but were not saved: {}",
                    schema.modId(), e.getMessage());
        }
        sync().ifPresent(sync -> toPlayers.accept(server, sync));
    }

    private ConfigValues running() {
        ConfigValues current = values;
        if (current == null) {
            throw new IllegalStateException(schema.modId() + ": no server is running, so there are no server settings");
        }
        return current;
    }
}
