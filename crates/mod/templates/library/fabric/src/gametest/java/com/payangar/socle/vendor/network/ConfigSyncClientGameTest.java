package com.payangar.socle.vendor.network;

import com.payangar.socle.vendor.config.ConfigValues;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.SettingsSource;
import com.payangar.socle.vendor.gui.screen.GeneratedConfigScreen;
import java.util.Map;
import java.util.Objects;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestDedicatedServerConnection;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestDedicatedServerContext;
import net.minecraft.network.chat.Component;
import net.minecraft.server.permissions.Permissions;
import net.minecraft.server.players.NameAndId;

/**
 * A client joins a dedicated server and receives its server settings; a server-only mod has its
 * server settings on the server. The config screen shows the settings the server sent, read-only,
 * to an operator too. A {@code fabric-client-gametest} entrypoint of the gametest mod.
 */
public final class ConfigSyncClientGameTest implements FabricClientGameTest {
    @Override
    public void runTest(ClientGameTestContext context) {
        ReceivedConfig received = NetworkTestMod.NETWORK.receivedConfig();
        ServerConfig serverConfig = NetworkTestMod.NETWORK.serverConfig();
        try (TestDedicatedServerContext server = context.worldBuilder().createServer()) {
            // The server's own value, as if written in its world's file, so that the screen cannot
            // pass by showing the defaults.
            server.runOnServer(s -> {
                ConfigValues values = serverConfig.values().orElseThrow();
                values.set(NetworkTestMod.RANGE, 16);
                serverConfig.update(s, values);
            });
            try (TestDedicatedServerConnection connection = server.connect()) {
                context.waitFor(client -> received.synced());
                check(16, context.computeOnClient(client -> received.get(NetworkTestMod.RANGE)), "range received at join");
                ServerConfig serverOnly = NetworkTestMod.SERVER_NETWORK.serverConfig();
                check(4, server.computeOnServer(s -> serverOnly.get(NetworkTestMod.SERVER_RANGE)), "range of a server-only mod");

                NameAndId player = server.computeOnServer(s -> connection.getServerPlayer().nameAndId());
                server.runOnServer(s -> s.getPlayerList().op(player));
                context.waitFor(client -> Objects.requireNonNull(client.player).permissions()
                        .hasPermission(Permissions.COMMANDS_GAMEMASTER));

                SettingsSource source = context.computeOnClient(client -> ServerSettings.of(NetworkTestMod.NETWORK));
                check(false, source.editable(), "server settings editable by an operator on a dedicated server");
                check(16, source.values().get(NetworkTestMod.RANGE), "range shown by the config screen");
                context.setScreen(() -> new GeneratedConfigScreen(Component.literal("Server settings"), null,
                        NetworkTestMod.SCHEMA, Map.of(Scope.SERVER, source)));
                context.waitTick();
                check(false, context.computeOnClient(client -> ((GeneratedConfigScreen) client.gui.screen()).rows()
                        .stream()
                        .anyMatch(row -> row.control().active)), "a server setting's control active");
                context.setScreen(() -> null);
            }
        }
    }

    private static void check(Object expected, Object actual, String what) {
        if (!Objects.equals(expected, actual)) {
            throw new AssertionError(what + ": expected " + expected + ", got " + actual);
        }
    }
}
