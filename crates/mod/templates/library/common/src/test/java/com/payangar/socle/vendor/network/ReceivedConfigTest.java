package com.payangar.socle.vendor.network;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.payangar.socle.vendor.config.ConfigSchema;
import com.payangar.socle.vendor.config.ConfigValues;
import com.payangar.socle.vendor.config.ModSide;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.Setting;
import org.junit.jupiter.api.Test;

class ReceivedConfigTest {
    private final ConfigSchema.Builder config = ConfigSchema.builder("demo", ModSide.BOTH_OPTIONAL);
    private final Setting<Integer> range = config.define(Setting.integer("range", Scope.SERVER, 8, 1, 64));
    private final Setting<Boolean> pvp = config.define(Setting.bool("pvp", Scope.SERVER, false));
    private final ModNetwork network = new ModNetwork(config.build());
    private final ReceivedConfig received = network.receivedConfig();

    @Test
    void theDefaultsHoldUntilTheServerSendsItsSettings() {
        assertFalse(received.synced());
        assertEquals(8, received.get(range));

        received.receive(network.payloads().sync("{\"range\": 16, \"pvp\": true}"));

        assertTrue(received.synced());
        assertEquals(16, received.get(range));
        assertEquals(true, received.get(pvp));
    }

    @Test
    void leavingTheServerBringsBackTheDefaults() {
        received.receive(network.payloads().sync("{\"range\": 16}"));
        received.reset();

        assertFalse(received.synced());
        assertEquals(8, received.get(range));
    }

    @Test
    void aValueTheClientCannotReadTakesItsDefault() {
        received.receive(network.payloads().sync("{\"range\": \"far\", \"pvp\": true, \"newer\": 1}"));

        assertTrue(received.synced());
        assertEquals(8, received.get(range));
        assertEquals(true, received.get(pvp));
    }

    @Test
    void theValuesShownAreACopyOfTheReceivedOnes() {
        received.receive(network.payloads().sync("{\"range\": 16}"));

        ConfigValues shown = received.values();
        assertEquals(16, shown.get(range));
        shown.set(range, 32);

        assertEquals(16, received.get(range));
    }
}
