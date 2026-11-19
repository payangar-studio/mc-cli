package {{package}}.neoforge;

import {{package}}.CommonEntrypoint;
import net.neoforged.fml.common.Mod;

/** NeoForge's entrypoint, on both sides. */
@Mod(CommonEntrypoint.MOD_ID)
public final class NeoForgeEntrypoint {
    public NeoForgeEntrypoint() {
        CommonEntrypoint.init();
    }
}
