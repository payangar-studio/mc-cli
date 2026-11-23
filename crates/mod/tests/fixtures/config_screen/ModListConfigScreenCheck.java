package com.payangar.swiftblades.neoforge.client;

import com.payangar.swiftblades.CommonEntrypoint;
import com.payangar.swiftblades.vendor.gui.screen.GeneratedConfigScreen;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.LoadingOverlay;
import net.minecraft.client.gui.screens.Screen;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.gui.IConfigScreenFactory;
import net.neoforged.neoforge.common.NeoForge;
import org.slf4j.LoggerFactory;

/**
 * The mod's config screen, as NeoForge's mod list opens it: the config screen extension of the
 * mod's container, once the client has loaded. NeoForge runs no client gametest, so the client
 * self-test carries this check and logs what it found: the screen's class, and whether it shows
 * the mod's own schema.
 */
@Mod(value = CommonEntrypoint.MOD_ID, dist = Dist.CLIENT)
public final class ModListConfigScreenCheck {
    private boolean done;

    public ModListConfigScreenCheck(ModContainer container) {
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Pre event) -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (done || minecraft.gui.overlay() instanceof LoadingOverlay) {
                return;
            }
            done = true;
            Screen parent = minecraft.gui.screen();
            Screen screen = IConfigScreenFactory.getForMod(container.getModInfo())
                    .map(factory -> factory.createScreen(container, parent))
                    .orElse(null);
            if (screen != null) {
                minecraft.gui.setScreen(screen);
            }
            boolean schema = screen instanceof GeneratedConfigScreen generated
                    && generated.schema() == CommonEntrypoint.SCHEMA;
            LoggerFactory.getLogger(ModListConfigScreenCheck.class)
                    .info("Config screen from the mod list: {}, showing CommonEntrypoint.SCHEMA: {}",
                            screen == null ? "none" : screen.getClass().getName(), schema);
        });
    }
}
