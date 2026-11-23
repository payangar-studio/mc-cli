package com.payangar.socle.vendor.network;

import com.payangar.socle.vendor.config.ConfigSchema;
import com.payangar.socle.vendor.config.ModSide;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.Setting;
import net.fabricmc.api.ModInitializer;

/**
 * Two mods of the gametests with a server setting, one required on both sides and one server
 * only, their networks registered as a mod does: a {@code main} entrypoint of the gametest mod.
 */
public final class NetworkTestMod implements ModInitializer {
    /** The id its payloads and its file are named with. */
    static final String MOD_ID = "vendor_network_test";

    private static final ConfigSchema.Builder CONFIG = ConfigSchema.builder(MOD_ID, ModSide.BOTH_REQUIRED);
    static final Setting<Integer> RANGE = CONFIG.define(Setting.integer("range", Scope.SERVER, 8, 1, 64));
    static final ConfigSchema SCHEMA = CONFIG.build();
    static final ModNetwork NETWORK = new ModNetwork(SCHEMA);

    /** The id of a server-only mod, which has server settings and no payloads. */
    static final String SERVER_MOD_ID = "vendor_server_test";

    private static final ConfigSchema.Builder SERVER_CONFIG = ConfigSchema.builder(SERVER_MOD_ID, ModSide.SERVER);
    static final Setting<Integer> SERVER_RANGE =
            SERVER_CONFIG.define(Setting.integer("range", Scope.SERVER, 4, 1, 64));
    static final ModNetwork SERVER_NETWORK = new ModNetwork(SERVER_CONFIG.build());

    @Override
    public void onInitialize() {
        FabricNetwork.register(NETWORK);
        FabricNetwork.register(SERVER_NETWORK);
    }
}
