package com.payangar.socle.vendor.network;

import com.google.gson.JsonParser;
import com.payangar.socle.vendor.config.ConfigFile;
import com.payangar.socle.vendor.config.ConfigValues;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Objects;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestDedicatedServerConnection;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestDedicatedServerContext;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.players.NameAndId;
import net.minecraft.world.level.storage.LevelResource;

/**
 * A client joins a dedicated server and receives its server settings. A change from a player who
 * is not an operator is refused; the same change from an operator is applied, saved in the world,
 * and comes back to the client. A {@code fabric-client-gametest} entrypoint of the gametest mod.
 */
public final class ConfigSyncClientGameTest implements FabricClientGameTest {
    @Override
    public void runTest(ClientGameTestContext context) {
        ReceivedConfig received = NetworkTestMod.NETWORK.receivedConfig();
        ServerConfig serverConfig = NetworkTestMod.NETWORK.serverConfig();
        try (TestDedicatedServerContext server = context.worldBuilder().createServer();
                TestDedicatedServerConnection connection = server.connect()) {
            context.waitFor(client -> received.synced());
            check(8, context.computeOnClient(client -> received.get(NetworkTestMod.RANGE)), "range received at join");
            NameAndId player = server.computeOnServer(s -> connection.getServerPlayer().nameAndId());

            server.runOnServer(s -> s.getPlayerList().deop(player));
            context.waitFor(client -> !ServerConfig.canEdit(Objects.requireNonNull(client.player)));
            request(context, received, 16);
            connection.waitForServerboundPackets();
            check(8, server.computeOnServer(s -> serverConfig.get(NetworkTestMod.RANGE)), "range after a player's change");
            connection.waitForClientboundPackets();
            check(8, context.computeOnClient(client -> received.get(NetworkTestMod.RANGE)), "range received after a player's change");

            server.runOnServer(s -> s.getPlayerList().op(player));
            context.waitFor(client -> ServerConfig.canEdit(Objects.requireNonNull(client.player)));
            request(context, received, 16);
            context.waitFor(client -> received.get(NetworkTestMod.RANGE) == 16);
            check(16, server.computeOnServer(s -> serverConfig.get(NetworkTestMod.RANGE)), "range after an operator's change");
            check(16, server.computeOnServer(ConfigSyncClientGameTest::savedRange), "range saved in the world");
        }
    }

    /** Asks the server, from the client, to set the range to {@code range}. */
    private static void request(ClientGameTestContext context, ReceivedConfig received, int range) {
        boolean sent = context.computeOnClient(client -> {
            ConfigValues edited = received.edit();
            edited.set(NetworkTestMod.RANGE, range);
            return received.request(edited);
        });
        check(true, sent, "change sent");
    }

    private static int savedRange(MinecraftServer server) {
        Path file = ConfigFile.serverPath(server.getWorldPath(LevelResource.ROOT), NetworkTestMod.MOD_ID);
        try {
            return JsonParser.parseString(Files.readString(file)).getAsJsonObject().get("range").getAsInt();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    private static void check(Object expected, Object actual, String what) {
        if (!Objects.equals(expected, actual)) {
            throw new AssertionError(what + ": expected " + expected + ", got " + actual);
        }
    }
}
