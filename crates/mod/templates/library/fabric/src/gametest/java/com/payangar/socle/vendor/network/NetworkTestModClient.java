package com.payangar.socle.vendor.network;

import net.fabricmc.api.ClientModInitializer;

/** The client side of {@link NetworkTestMod}: a {@code client} entrypoint of the gametest mod. */
public final class NetworkTestModClient implements ClientModInitializer {
    @Override
    public void onInitializeClient() {
        FabricNetworkClient.register(NetworkTestMod.NETWORK);
    }
}
