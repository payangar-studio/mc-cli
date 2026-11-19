package com.payangar.socle.vendor.network;

import com.payangar.socle.vendor.config.ConfigSchema;
import com.payangar.socle.vendor.config.ModSide;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.Setting;
import net.fabricmc.api.ModInitializer;

/**
 * A mod of the gametests with a server setting, its network registered as a mod does: a
 * {@code main} entrypoint of the gametest mod.
 */
public final class NetworkTestMod implements ModInitializer {
    /** The id its payloads and its file are named with. */
    static final String MOD_ID = "vendor_network_test";

    private static final ConfigSchema.Builder CONFIG = ConfigSchema.builder(MOD_ID, ModSide.BOTH_REQUIRED);
    static final Setting<Integer> RANGE = CONFIG.define(Setting.integer("range", Scope.SERVER, 8, 1, 64));
    static final ModNetwork NETWORK = new ModNetwork(CONFIG.build());

    @Override
    public void onInitialize() {
        FabricNetwork.register(NETWORK);
    }
}
