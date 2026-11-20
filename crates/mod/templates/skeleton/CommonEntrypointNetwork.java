package {{package}};

import {{package}}.vendor.config.ConfigSchema;
import {{package}}.vendor.config.ModSide;
import {{package}}.vendor.network.ModNetwork;

/** Code shared by every loader and both sides. */
public final class CommonEntrypoint {
    public static final String MOD_ID = "{{id}}";

    /** The mod's settings: define each one here, before {@link #NETWORK} builds the schema. */
    private static final ConfigSchema.Builder CONFIG = ConfigSchema.builder(MOD_ID, ModSide.{{mod_side}});

    /**
     * What the shared library does over the network for this mod: the check of the other side, the
     * sync of the server settings. Each loader's entrypoints register it.
     */
    public static final ModNetwork NETWORK = new ModNetwork(CONFIG.build());

    private CommonEntrypoint() {}

    /** Called once by each loader when the mod loads. */
    public static void init() {}
}
