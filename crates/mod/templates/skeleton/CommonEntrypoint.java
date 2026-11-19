package {{package}};

/** Code shared by every loader and both sides. */
public final class CommonEntrypoint {
    public static final String MOD_ID = "{{id}}";

    private CommonEntrypoint() {}

    /** Called once by each loader when the mod loads. */
    public static void init() {}
}
