package {{package}}.neoforge.client;

import {{package}}.CommonEntrypoint;
import {{package}}.client.ClientEntrypoint;
import {{package}}.vendor.neoforge.NeoForgeConfigScreen;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;

/** NeoForge's client entrypoint, never loaded on a dedicated server. */
@Mod(value = CommonEntrypoint.MOD_ID, dist = Dist.CLIENT)
public final class NeoForgeClientEntrypoint {
    public NeoForgeClientEntrypoint(ModContainer container) {
        NeoForgeConfigScreen.register(container);
        ClientEntrypoint.init();
    }
}
