package com.payangar.socle.vendor.network;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.payangar.socle.vendor.config.ConfigFile;
import com.payangar.socle.vendor.config.ConfigSchema;
import com.payangar.socle.vendor.config.ConfigValues;
import com.payangar.socle.vendor.config.ModSide;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.Setting;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.storage.LevelResource;
import net.neoforged.testframework.junit.EphemeralTestServerProvider;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;

/**
 * The server side of the sync, on a server of the game: the world's file, a change made in single
 * player, and the settings sent to players, read by a client.
 */
@ExtendWith(EphemeralTestServerProvider.class)
class ServerConfigTest {
    private static final String MOD_ID = "vendor_network_test";

    private final ConfigSchema.Builder config = ConfigSchema.builder(MOD_ID, ModSide.BOTH_REQUIRED);
    private final Setting<Integer> range = config.define(Setting.integer("range", Scope.SERVER, 8, 1, 64));
    private final Setting<Boolean> pvp = config.define(Setting.bool("pvp", Scope.SERVER, false));
    private final Setting<String> motd = config.define(Setting.string("motd", Scope.SERVER, ""));
    private final ModNetwork network = new ModNetwork(config.build());
    private final ServerConfig serverConfig = network.serverConfig();
    private final List<ConfigPayloads.Sync> sent = new ArrayList<>();

    ServerConfigTest() {
        serverConfig.connect((server, sync) -> sent.add(sync));
    }

    @AfterEach
    void stop() {
        serverConfig.stop();
    }

    private static Path file(MinecraftServer server) {
        return ConfigFile.serverPath(server.getWorldPath(LevelResource.ROOT), MOD_ID);
    }

    /** Starts the server settings from a fresh file. */
    private void start(MinecraftServer server) throws IOException {
        Files.deleteIfExists(file(server));
        serverConfig.start(server);
    }

    /** The running settings with {@code setting} set to {@code value}, as a config screen edits them. */
    private <T> ConfigValues edited(Setting<T> setting, T value) {
        ConfigValues values = serverConfig.values().orElseThrow();
        values.set(setting, value);
        return values;
    }

    private static JsonObject read(Path file) throws IOException {
        return JsonParser.parseString(Files.readString(file)).getAsJsonObject();
    }

    @Test
    void theSettingsOfTheWorldAreLoadedWhenTheServerStarts(MinecraftServer server) throws IOException {
        Files.createDirectories(file(server).getParent());
        Files.writeString(file(server), "{\"range\": 24}");

        serverConfig.start(server);

        assertEquals(24, serverConfig.get(range));
        assertEquals(false, serverConfig.get(pvp));
    }

    @Test
    void thereAreNoServerSettingsWithoutARunningServer() {
        assertThrows(IllegalStateException.class, () -> serverConfig.get(range));
        assertTrue(serverConfig.values().isEmpty());
    }

    @Test
    void theValuesGivenOutAreACopy(MinecraftServer server) throws IOException {
        start(server);

        serverConfig.values().orElseThrow().set(range, 16);

        assertEquals(8, serverConfig.get(range));
    }

    @Test
    void anUpdateIsAppliedSavedAndSentToPlayers(MinecraftServer server) throws IOException {
        start(server);

        serverConfig.update(server, edited(range, 16));

        assertEquals(16, serverConfig.get(range));
        assertEquals(16, read(file(server)).get("range").getAsInt());
        assertEquals(1, sent.size());
        ReceivedConfig client = network.receivedConfig();
        client.receive(sent.get(0));
        assertEquals(16, client.get(range));
    }

    @Test
    void anUpdateKeepsTheKeysOfTheFileNoSettingDeclares(MinecraftServer server) throws IOException {
        Files.createDirectories(file(server).getParent());
        Files.writeString(file(server), "{\"range\": 8, \"newer\": 1}");
        serverConfig.start(server);

        serverConfig.update(server, edited(pvp, true));

        assertEquals(1, read(file(server)).get("newer").getAsInt());
        assertEquals(true, read(file(server)).get("pvp").getAsBoolean());
    }

    @Test
    void anUpdateAfterTheServerStoppedIsDropped(MinecraftServer server) throws IOException {
        start(server);
        ConfigValues values = edited(range, 16);
        serverConfig.stop();

        serverConfig.update(server, values);

        assertTrue(serverConfig.values().isEmpty());
        assertEquals(List.of(), sent);
    }

    @Test
    void theSettingsSentToPlayersAreTheRunningOnes(MinecraftServer server) throws IOException {
        start(server);
        serverConfig.update(server, edited(pvp, true));

        assertEquals(
                JsonParser.parseString("{\"range\": 8, \"pvp\": true, \"motd\": \"\"}"),
                JsonParser.parseString(serverConfig.sync().orElseThrow().values()));
    }

    @Test
    void settingsTooLongForTheirPayloadAreNotSent(MinecraftServer server) throws IOException {
        Files.createDirectories(file(server).getParent());
        Files.writeString(file(server), "{\"motd\": \"" + "x".repeat(ConfigPayloads.MAX_SYNC) + "\"}");
        serverConfig.start(server);

        assertTrue(serverConfig.sync().isEmpty());
    }
}
