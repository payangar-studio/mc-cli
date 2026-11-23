package {{package}};

import {{package}}.vendor.config.ConfigSchema;
import {{package}}.vendor.config.ModSide;

/** Code shared by every loader and both sides. */
public final class CommonEntrypoint {
    public static final String MOD_ID = "{{id}}";

    /** The mod's settings: define each one here, before {@link #SCHEMA} builds them. */
    private static final ConfigSchema.Builder CONFIG = ConfigSchema.builder(MOD_ID, ModSide.{{mod_side}});

    /** The mod's settings, which the client entrypoint gives to the generated config screen. */
    public static final ConfigSchema SCHEMA = CONFIG.build();

    private CommonEntrypoint() {}

    /** Called once by each loader when the mod loads. */
    public static void init() {}
}
