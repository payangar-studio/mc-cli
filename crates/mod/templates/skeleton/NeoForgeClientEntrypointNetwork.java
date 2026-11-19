package {{package}}.neoforge.client;

import {{package}}.CommonEntrypoint;
import {{package}}.client.ClientEntrypoint;
import {{package}}.vendor.network.client.NeoForgeNetworkClient;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;

/** NeoForge's client entrypoint, never loaded on a dedicated server. */
@Mod(value = CommonEntrypoint.MOD_ID, dist = Dist.CLIENT)
public final class NeoForgeClientEntrypoint {
    public NeoForgeClientEntrypoint(IEventBus modBus) {
        NeoForgeNetworkClient.register(CommonEntrypoint.NETWORK, modBus);
        ClientEntrypoint.init();
    }
}
