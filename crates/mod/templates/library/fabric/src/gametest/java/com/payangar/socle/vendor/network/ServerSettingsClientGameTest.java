package com.payangar.socle.vendor.network;

import com.google.gson.JsonParser;
import com.payangar.socle.vendor.config.ConfigFile;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.SettingsSource;
import com.payangar.socle.vendor.gui.screen.GeneratedConfigScreen;
import com.payangar.socle.vendor.gui.screen.SettingRow;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Map;
import java.util.Objects;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.fabricmc.fabric.api.client.gametest.v1.world.TestWorldSave;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.network.chat.Component;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.storage.LevelResource;

/**
 * In single player, the config screen changes the world's server settings: a setting moved with the
 * mouse applies on the integrated server, is saved in the world's file, comes back to the client,
 * and is there again once the world is reopened. A {@code fabric-client-gametest} entrypoint of the
 * gametest mod.
 */
public final class ServerSettingsClientGameTest implements FabricClientGameTest {
    /** Longer than the game's double-click time, 250 ms. */
    private static final int DOUBLE_CLICK_TICKS = 6;

    @Override
    public void runTest(ClientGameTestContext context) {
        ServerConfig serverConfig = NetworkTestMod.NETWORK.serverConfig();
        ReceivedConfig received = NetworkTestMod.NETWORK.receivedConfig();
        TestWorldSave save;
        try (TestSingleplayerContext world = context.worldBuilder().create()) {
            save = world.getWorldSave();
            context.waitFor(client -> received.synced());
            check(8, world.getServer().computeOnServer(s -> serverConfig.get(NetworkTestMod.RANGE)), "range of a new world");

            openScreen(context);
            check(true, context.computeOnClient(client -> rangeRow(client.gui.screen()).control().active),
                    "the range's control active in single player");
            // Right of the slider's track, over its value: the maximum.
            AbstractWidget slider = context.computeOnClient(client -> rangeRow(client.gui.screen()).control());
            click(context, slider, 0.99, 0.5);
            context.clickScreenButton("gui.done");

            world.getServer().waitFor(s -> serverConfig.get(NetworkTestMod.RANGE) == 64);
            check(64, world.getServer().computeOnServer(ServerSettingsClientGameTest::savedRange), "range saved in the world");
            context.waitFor(client -> received.get(NetworkTestMod.RANGE) == 64);
        }
        // Read back: the reopened world loads the saved value, which the screen shows, editable.
        try (TestSingleplayerContext world = save.open()) {
            check(64, world.getServer().computeOnServer(s -> serverConfig.get(NetworkTestMod.RANGE)), "range after reopening");
            SettingsSource source = context.computeOnClient(client -> ServerSettings.of(NetworkTestMod.NETWORK));
            check(true, source.editable(), "server settings editable in single player");
            check(64, source.values().get(NetworkTestMod.RANGE), "range shown after reopening");
        }
    }

    private static void openScreen(ClientGameTestContext context) {
        context.setScreen(() -> new GeneratedConfigScreen(Component.literal("Server settings"), null,
                NetworkTestMod.SCHEMA, Map.of(Scope.SERVER, ServerSettings.of(NetworkTestMod.NETWORK))));
        context.waitTick();
    }

    private static SettingRow rangeRow(Object screen) {
        return ((GeneratedConfigScreen) screen).rows().stream()
                .filter(row -> row.setting() == NetworkTestMod.RANGE)
                .findFirst()
                .orElseThrow(() -> new AssertionError("no row for the range"));
    }

    /** A left click on {@code widget} at the fractions {@code fx} and {@code fy} of its size. */
    private static void click(ClientGameTestContext context, AbstractWidget widget, double fx, double fy) {
        double scale = context.computeOnClient(mc -> mc.getWindow().getScreenWidth() / (double) mc.getWindow().getGuiScaledWidth());
        double x = widget.getX() + (widget.getWidth() - 1) * fx + 0.5;
        double y = widget.getY() + (widget.getHeight() - 1) * fy + 0.5;
        context.getInput().setCursorPos(x * scale, y * scale);
        context.getInput().pressMouse(1);
        context.waitTicks(DOUBLE_CLICK_TICKS);
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
