package com.payangar.socle.vendor.network;

import com.payangar.socle.vendor.config.ConfigValues;
import com.payangar.socle.vendor.config.SettingsSource;
import java.util.Optional;
import net.minecraft.client.Minecraft;
import net.minecraft.client.server.IntegratedServer;

/**
 * The server settings a config screen shows. In single player, those of the world, which the
 * player who owns it changes: the screen edits a copy, and saving hands it to the integrated
 * server, which applies it, writes {@code serverconfig/<modid>.json} of the world and sends it to
 * its players. On a server, those it sent, read-only for every player, operators included: a
 * dedicated server's settings are changed in its world's file. Register it from the mod's client
 * entrypoint:
 *
 * <pre>{@code
 * ConfigScreens.serverSettings(() -> ServerSettings.of(MyMod.NETWORK));
 * }</pre>
 */
public final class ServerSettings {
    private ServerSettings() {}

    /** The server settings of {@code network}'s mod as they are now; called on the client thread. */
    public static SettingsSource of(ModNetwork network) {
        IntegratedServer server = Minecraft.getInstance().getSingleplayerServer();
        ServerConfig config = network.serverConfig();
        Optional<ConfigValues> world = server == null ? Optional.empty() : config.values();
        if (world.isEmpty()) {
            return SettingsSource.readOnly(network.receivedConfig().values());
        }
        ConfigValues values = world.get();
        return new SettingsSource() {
            @Override
            public ConfigValues values() {
                return values;
            }

            @Override
            public boolean editable() {
                return true;
            }

            @Override
            public void save() {
                ConfigValues changed = values.copy();
                server.execute(() -> config.update(server, changed));
            }
        };
    }
}
