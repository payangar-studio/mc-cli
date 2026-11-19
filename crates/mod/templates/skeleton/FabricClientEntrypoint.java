package {{package}}.fabric.client;

import {{package}}.client.ClientEntrypoint;
import net.fabricmc.api.ClientModInitializer;

/** Fabric's client entrypoint, named in the generated fabric.mod.json. */
public final class FabricClientEntrypoint implements ClientModInitializer {
    @Override
    public void onInitializeClient() {
        ClientEntrypoint.init();
    }
}
