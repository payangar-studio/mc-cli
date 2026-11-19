package com.payangar.socle.vendor.network;

import com.google.gson.JsonElement;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.payangar.socle.vendor.config.ConfigFile;
import com.payangar.socle.vendor.config.ConfigSchema;
import com.payangar.socle.vendor.config.ConfigValues;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.Setting;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Path;
import java.util.List;
import java.util.Optional;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.permissions.PermissionSet;
import net.minecraft.server.permissions.Permissions;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.storage.LevelResource;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * The server settings of the running server, which owns them: loaded from the folder of its world
 * when it starts, changed by operators only, sent to its players. In singleplayer the integrated
 * server plays that role. Used on the server thread.
 */
public final class ServerConfig {
    private final ConfigSchema schema;
    private final ConfigPayloads payloads;
    private final Logger logger;
    private volatile ConfigValues values;
    private volatile Path file;

    ServerConfig(ConfigSchema schema, ConfigPayloads payloads) {
        this.schema = schema;
        this.payloads = payloads;
        this.logger = LoggerFactory.getLogger(schema.modId());
    }

    /**
     * Whether {@code player} may change server settings: the permission vanilla asks to change a
     * game rule, which an operator has. The client checks it on its own player to show the values
     * read-only; the server checks it again on every change.
     */
    public static boolean canEdit(Player player) {
        return canEdit(player.permissions());
    }

    static boolean canEdit(PermissionSet permissions) {
        return permissions.hasPermission(Permissions.COMMANDS_GAMEMASTER);
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

    /**
     * The value of {@code setting} on the running server.
     *
     * @throws IllegalStateException when no server is running
     */
    public <T> T get(Setting<T> setting) {
        return running().get(setting);
    }

    /** The settings to send to a player. */
    public ConfigPayloads.Sync sync() {
        return payloads.sync(running().settingsJson().toString());
    }

    /**
     * Applies the change {@code player} asks for, when they may change server settings and every
     * value of the change is valid: a change is applied whole or not at all. The settings are then
     * saved to the world's file.
     *
     * @return the settings to send to every player, empty when the change is refused
     */
    public Optional<ConfigPayloads.Sync> change(ServerPlayer player, ConfigPayloads.Change change) {
        return change(player.getGameProfile().name(), player.permissions(), change);
    }

    /** {@link #change(ServerPlayer, ConfigPayloads.Change)} from the player named {@code name}. */
    Optional<ConfigPayloads.Sync> change(String name, PermissionSet permissions, ConfigPayloads.Change change) {
        if (!canEdit(permissions)) {
            logger.warn("{} tried to change server settings of {} without the permission of an operator", name,
                    schema.modId());
            return Optional.empty();
        }
        ConfigValues changed = running().copy();
        List<String> problems;
        try {
            JsonElement json = JsonParser.parseString(change.changes());
            problems = json.isJsonObject() ? changed.read(json.getAsJsonObject()) : List.of("not a JSON object");
        } catch (JsonParseException e) {
            problems = List.of("not a JSON object (" + e.getMessage() + ")");
        }
        if (!problems.isEmpty()) {
            logger.warn("Refused the change of server settings of {} from {}: {}", schema.modId(), name,
                    String.join("; ", problems));
            return Optional.empty();
        }
        values = changed;
        try {
            ConfigFile.save(changed, file);
        } catch (IOException e) {
            logger.error("The server settings of {} changed by {} apply until the server stops, but were not"
                    + " saved: {}", schema.modId(), name, e.getMessage());
        }
        logger.info("{} changed server settings of {}: {}", name, schema.modId(), change.changes());
        return Optional.of(sync());
    }

    private ConfigValues running() {
        ConfigValues current = values;
        if (current == null) {
            throw new IllegalStateException(schema.modId() + ": no server is running, so there are no server settings");
        }
        return current;
    }
}
