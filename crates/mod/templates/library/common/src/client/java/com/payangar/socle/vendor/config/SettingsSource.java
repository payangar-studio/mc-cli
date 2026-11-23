package com.payangar.socle.vendor.config;

import java.io.IOException;
import java.util.Objects;

/**
 * The settings of one scope as a config screen sees them: their current values, whether the
 * player may change them, and how a change is kept.
 */
public interface SettingsSource {
    /**
     * The values a screen shows and changes in place. Those of the client settings are the ones in
     * use, so the game sees a change at once; a source may instead hand out a copy, which reaches
     * the game when {@link #save} keeps it (the server settings in single player).
     */
    ConfigValues values();

    /** Whether the player may change them; a screen shows them read-only otherwise. */
    boolean editable();

    /** Keeps the values after a screen changed them. */
    void save() throws IOException;

    /** Values shown and never changed. */
    static SettingsSource readOnly(ConfigValues values) {
        Objects.requireNonNull(values, "values");
        return new SettingsSource() {
            @Override
            public ConfigValues values() {
                return values;
            }

            @Override
            public boolean editable() {
                return false;
            }

            @Override
            public void save() {}
        };
    }
}
