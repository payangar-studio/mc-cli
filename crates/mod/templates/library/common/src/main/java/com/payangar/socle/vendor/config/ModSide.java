package com.payangar.socle.vendor.config;

/** The side of the host mod, as {@code side} of its {@code mod.toml}. */
public enum ModSide {
    /** Client only: the mod has client settings only. */
    CLIENT,
    SERVER,
    BOTH_REQUIRED,
    BOTH_OPTIONAL;

    /** Whether the mod may declare settings of the server scope. */
    public boolean allowsServerScope() {
        return this != CLIENT;
    }
}
