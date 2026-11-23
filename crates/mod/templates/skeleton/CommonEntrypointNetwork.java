package {{package}};

import {{package}}.vendor.config.ConfigSchema;
import {{package}}.vendor.config.ModSide;
import {{package}}.vendor.network.ModNetwork;

/** Code shared by every loader and both sides. */
public final class CommonEntrypoint {
    public static final String MOD_ID = "{{id}}";

    /** The mod's settings: define each one here, before {@link #SCHEMA} builds them. */
    private static final ConfigSchema.Builder CONFIG = ConfigSchema.builder(MOD_ID, ModSide.{{mod_side}});

    /**
     * The mod's settings, which {@link #NETWORK} syncs and the client entrypoint gives to the
     * generated config screen.
     */
    public static final ConfigSchema SCHEMA = CONFIG.build();

    /**
     * What the shared library does over the network for this mod: the check of the other side, the
     * sync of the server settings. Each loader's entrypoints register it.
     */
    public static final ModNetwork NETWORK = new ModNetwork(SCHEMA);

    private CommonEntrypoint() {}

    /** Called once by each loader when the mod loads. */
    public static void init() {}
}
