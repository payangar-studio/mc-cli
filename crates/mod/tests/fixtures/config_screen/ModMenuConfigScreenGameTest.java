package com.payangar.swiftblades.gametest;

import com.payangar.swiftblades.CommonEntrypoint;
import com.payangar.swiftblades.vendor.gui.screen.GeneratedConfigScreen;
import com.terraformersmc.modmenu.ModMenu;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.minecraft.client.gui.screens.Screen;

/**
 * The mod's config screen, as Mod Menu opens it from its list: through the `modmenu` entrypoint
 * of the mod's fabric.mod.json, which the shared library provides.
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
        context.setScreen(() -> null);
    }
}
