package {{package}}.neoforge.client;

import {{package}}.CommonEntrypoint;
import {{package}}.client.ClientEntrypoint;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.fml.common.Mod;

/** NeoForge's client entrypoint, never loaded on a dedicated server. */
@Mod(value = CommonEntrypoint.MOD_ID, dist = Dist.CLIENT)
public final class NeoForgeClientEntrypoint {
    public NeoForgeClientEntrypoint() {
        ClientEntrypoint.init();
    }
}
