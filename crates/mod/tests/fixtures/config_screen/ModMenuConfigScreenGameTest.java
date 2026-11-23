package com.payangar.swiftblades.gametest;

import com.payangar.swiftblades.CommonEntrypoint;
import com.payangar.swiftblades.vendor.config.Scope;
import com.payangar.swiftblades.vendor.gui.screen.GeneratedConfigScreen;
import com.terraformersmc.modmenu.ModMenu;
import java.util.List;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.minecraft.client.gui.screens.Screen;

/**
 * The mod's config screen, as Mod Menu opens it from its list: through the `modmenu` entrypoint
 * of the mod's fabric.mod.json, which the shared library provides. Its server settings, which the
 * skeleton's client entrypoint wires, are read-only without a world and editable in single player.
 */
public class ModMenuConfigScreenGameTest implements FabricClientGameTest {
    @Override
    public void runTest(ClientGameTestContext context) {
        Screen screen = context.computeOnClient(mc -> ModMenu.getConfigScreen(CommonEntrypoint.MOD_ID, mc.gui.screen()));
        if (!(screen instanceof GeneratedConfigScreen generated)) {
            throw new AssertionError("Mod Menu gave " + screen + " as the config screen of " + CommonEntrypoint.MOD_ID);
        }
        if (generated.schema() != CommonEntrypoint.SCHEMA) {
            throw new AssertionError("the config screen shows another schema than CommonEntrypoint.SCHEMA");
        }
        context.setScreen(() -> screen);
        context.waitTick();
        checkServerControls(context, false, "without a world");
        context.setScreen(() -> null);

        try (TestSingleplayerContext world = context.worldBuilder().create()) {
            context.setScreen(() -> ModMenu.getConfigScreen(CommonEntrypoint.MOD_ID, null));
            context.waitTick();
            checkServerControls(context, true, "in single player");
            context.setScreen(() -> null);
        }
    }

    /** Whether the server settings' controls of the screen shown are {@code active}. */
    private static void checkServerControls(ClientGameTestContext context, boolean active, String where) {
        List<Boolean> found = context.computeOnClient(mc -> {
            GeneratedConfigScreen generated = (GeneratedConfigScreen) mc.gui.screen();
            if (generated.scope() != Scope.SERVER) {
                throw new AssertionError("the config screen shows " + generated.scope() + " settings, not the server's");
            }
            return generated.rows().stream().map(row -> row.control().active).toList();
        });
        if (!found.equals(List.of(active))) {
            throw new AssertionError("server controls active " + where + ": expected [" + active + "], got " + found);
        }
    }
}
