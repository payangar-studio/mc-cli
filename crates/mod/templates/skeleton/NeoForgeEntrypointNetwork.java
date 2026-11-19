package {{package}}.neoforge;

import {{package}}.CommonEntrypoint;
import {{package}}.vendor.network.NeoForgeNetwork;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;

/** NeoForge's entrypoint, on both sides. */
@Mod(CommonEntrypoint.MOD_ID)
public final class NeoForgeEntrypoint {
    public NeoForgeEntrypoint(IEventBus modBus) {
        NeoForgeNetwork.register(CommonEntrypoint.NETWORK, modBus);
        CommonEntrypoint.init();
    }
}
