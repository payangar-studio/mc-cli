package {{package}}.fabric.client;

import {{package}}.CommonEntrypoint;
import {{package}}.client.ClientEntrypoint;
import {{package}}.vendor.network.FabricNetworkClient;
import net.fabricmc.api.ClientModInitializer;

/** Fabric's client entrypoint, named in the generated fabric.mod.json. */
public final class FabricClientEntrypoint implements ClientModInitializer {
    @Override
    public void onInitializeClient() {
        FabricNetworkClient.register(CommonEntrypoint.NETWORK);
        ClientEntrypoint.init();
    }
}
