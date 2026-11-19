package com.payangar.socle.vendor.gui;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.payangar.socle.vendor.config.ClientSettings;
import com.payangar.socle.vendor.config.ConfigSchema;
import com.payangar.socle.vendor.config.ConfigValues;
import com.payangar.socle.vendor.config.ModSide;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.Setting;
import com.payangar.socle.vendor.config.SettingsSource;
import com.payangar.socle.vendor.gui.screen.GeneratedConfigScreen;
import com.payangar.socle.vendor.gui.screen.SettingRow;
import com.payangar.socle.vendor.gui.widget.Slider;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.screenshot.TestScreenshotComparisonAlgorithm;
import net.fabricmc.fabric.api.client.gametest.v1.screenshot.TestScreenshotComparisonOptions;
import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.ModContainer;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.network.chat.Component;

/**
 * The shared library's config screens on a real client: the generated screen and the widget
 * states compared with their reference renders, and a setting changed with the mouse and the
 * keyboard reaching the JSON file. References are renders of CI, like every screenshot test.
 */
public class ConfigScreenClientGameTest implements FabricClientGameTest {
    enum Quality { FAST, FANCY }

    enum Shape { CIRCLE, SQUARE, TRIANGLE, HEXAGON, STAR }

    /** Long enough for every hover animation to settle. */
    private static final int SETTLE_TICKS = 10;
    /** Longer than the game's double-click time, 250 ms. */
    private static final int DOUBLE_CLICK_TICKS = 6;

    @Override
    public void runTest(ClientGameTestContext context) {
        String modId = hostModId();
        ConfigSchema.Builder builder = ConfigSchema.builder(modId, ModSide.BOTH_REQUIRED);
        Setting<Boolean> enabled = builder.define(Setting.bool("enabled", Scope.CLIENT, true));
        Setting<Integer> volume = builder.define(Setting.integer("volume", Scope.CLIENT, 70, 0, 100));
        builder.define(Setting.decimal("opacity", Scope.CLIENT, 0.5, 0, 1));
        builder.define(Setting.choice("quality", Scope.CLIENT, Quality.FANCY));
        builder.define(Setting.choice("shape", Scope.CLIENT, Shape.SQUARE));
        Setting<String> greeting = builder.define(Setting.string("greeting", Scope.CLIENT, "Hello"));
        builder.define(Setting.colour("tint", Scope.CLIENT, 0xFF3F7FE8, false));
        builder.define(Setting.integer("range", Scope.SERVER, 8, 1, 64));
        builder.define(Setting.bool("pvp", Scope.SERVER, false));
        ConfigSchema schema = builder.build();

        Path file = context.computeOnClient(mc -> mc.gameDirectory.toPath().resolve("vendor-test").resolve(modId + ".json"));
        delete(file);
        ClientSettings client = ClientSettings.at(schema, file);
        SettingsSource server = SettingsSource.readOnly(new ConfigValues(schema, Scope.SERVER));

        context.setScreen(() -> new GeneratedConfigScreen(
                Component.literal("Config screen"), null, schema, Map.of(Scope.CLIENT, client, Scope.SERVER, server)));
        context.getInput().setCursorPos(1, 1);
        context.waitTicks(SETTLE_TICKS);
        context.assertScreenshotEquals(reference("vendor_config_screen"));

        // The mouse turns a toggle off and moves a slider to the middle; the keyboard types.
        click(context, control(context, enabled), 0.5, 0.5);
        Slider slider = (Slider) control(context, volume);
        double[] middle = context.computeOnClient(mc -> new double[] {
            slider.trackX() + 3 + (slider.trackWidth() - 6) / 2.0, slider.getY() + slider.getHeight() / 2.0
        });
        clickAt(context, middle[0], middle[1]);
        click(context, control(context, greeting), 0.95, 0.5);
        context.getInput().typeChars(" world");
        context.waitTick();
        context.clickScreenButton("gui.done");
        context.waitTick();

        JsonObject json = read(file);
        assertEquals(false, json.get("enabled").getAsBoolean(), "enabled in " + json);
        assertEquals(50, json.get("volume").getAsInt(), "volume in " + json);
        assertEquals("Hello world", json.get("greeting").getAsString(), "greeting in " + json);

        // The search keeps the rows whose label matches; the server tab is read-only.
        context.setScreen(() -> new GeneratedConfigScreen(
                Component.literal("Config screen"), null, schema, Map.of(Scope.CLIENT, client, Scope.SERVER, server)));
        context.waitTick();
        click(context, context.computeOnClient(mc -> ((GeneratedConfigScreen) mc.gui.screen()).search()), 0.5, 0.5);
        context.getInput().typeChars("GREET");
        context.waitTick();
        assertEquals(
                List.of("greeting"),
                context.computeOnClient(mc -> ((GeneratedConfigScreen) mc.gui.screen()).rows().stream()
                        .filter(row -> row.visible)
                        .map(row -> row.setting().name())
                        .toList()),
                "rows matching the search");
        context.runOnClient(mc -> ((GeneratedConfigScreen) mc.gui.screen()).selectScope(Scope.SERVER));
        context.waitTick();
        assertEquals(
                List.of(false, false),
                context.computeOnClient(mc -> ((GeneratedConfigScreen) mc.gui.screen()).rows().stream()
                        .map(row -> row.control().active)
                        .toList()),
                "server controls active for a player who may not change them");

        // Every widget enabled and disabled, the mouse over the enabled button.
        context.runOnClient(mc -> {
            mc.options.guiScale().set(1);
            mc.resizeGui();
        });
        context.setScreen(WidgetGallery::new);
        context.waitTick();
        AbstractWidget button = context.computeOnClient(mc -> ((WidgetGallery) mc.gui.screen()).button);
        hover(context, button, 0.5, 0.5);
        context.waitTicks(SETTLE_TICKS);
        context.assertScreenshotEquals(reference("vendor_widget_states"));

        // An open dropdown above the widgets under it.
        AbstractWidget dropdown = context.computeOnClient(mc -> ((WidgetGallery) mc.gui.screen()).dropdown);
        click(context, dropdown, 0.5, 0.5);
        context.getInput().setCursorPos(1, 1);
        context.waitTicks(SETTLE_TICKS);
        context.assertScreenshotEquals(reference("vendor_dropdown_open"));

        context.restoreDefaultGameOptions();
        context.runOnClient(Minecraft::resizeGui);
        context.setScreen(() -> null);
    }

    /**
     * The id of the mod hosting this copy of the library: its test mod is {@code <id>_gametest}.
     */
    private static String hostModId() {
        String suffix = "_gametest";
        return FabricLoader.getInstance().getAllMods().stream()
                .map(ModContainer::getMetadata)
                .map(meta -> meta.getId())
                .filter(id -> id.endsWith(suffix))
                .map(id -> id.substring(0, id.length() - suffix.length()))
                .findFirst()
                .orElseThrow(() -> new AssertionError("no test mod named <id>" + suffix));
    }

    /**
     * The comparison with the reference {@code name}, pixel for pixel: a hovered widget differs
     * from one at rest by a few pixels only, which the default tolerance would let through.
     */
    private static TestScreenshotComparisonOptions reference(String name) {
        return TestScreenshotComparisonOptions.of(name)
                .withAlgorithm(TestScreenshotComparisonAlgorithm.exact())
                .save()
                .disableCounterPrefix();
    }

    /** The widget of {@code setting} in the generated screen shown. */
    private static AbstractWidget control(ClientGameTestContext context, Setting<?> setting) {
        return context.computeOnClient(mc -> ((GeneratedConfigScreen) mc.gui.screen()).rows().stream()
                .filter(row -> row.setting() == setting)
                .map(SettingRow::control)
                .findFirst()
                .orElseThrow(() -> new AssertionError("no row for " + setting)));
    }

    /** Moves the mouse to the point of {@code widget} at the fractions {@code fx} and {@code fy}. */
    private static void hover(ClientGameTestContext context, AbstractWidget widget, double fx, double fy) {
        moveTo(context, widget.getX() + (widget.getWidth() - 1) * fx + 0.5, widget.getY() + (widget.getHeight() - 1) * fy + 0.5);
    }

    /** Moves the mouse to the GUI point ({@code x}, {@code y}). */
    private static void moveTo(ClientGameTestContext context, double x, double y) {
        double scale = context.computeOnClient(mc -> mc.getWindow().getScreenWidth() / (double) mc.getWindow().getGuiScaledWidth());
        context.getInput().setCursorPos(x * scale, y * scale);
    }

    /** A left click on {@code widget} at the fractions {@code fx} and {@code fy} of its size. */
    private static void click(ClientGameTestContext context, AbstractWidget widget, double fx, double fy) {
        hover(context, widget, fx, fy);
        context.getInput().pressMouse(1);
        // Two clicks closer than the game's double-click time are one double click.
        context.waitTicks(DOUBLE_CLICK_TICKS);
    }

    /** A left click at the GUI point ({@code x}, {@code y}). */
    private static void clickAt(ClientGameTestContext context, double x, double y) {
        moveTo(context, x, y);
        context.getInput().pressMouse(1);
        // Two clicks closer than the game's double-click time are one double click.
        context.waitTicks(DOUBLE_CLICK_TICKS);
    }

    private static void delete(Path file) {
        try {
            Files.deleteIfExists(file);
        } catch (IOException e) {
            throw new AssertionError(e);
        }
    }

    private static JsonObject read(Path file) {
        try {
            return JsonParser.parseString(Files.readString(file)).getAsJsonObject();
        } catch (IOException e) {
            throw new AssertionError(file + " was not written", e);
        }
    }

    private static void assertEquals(Object expected, Object actual, String what) {
        if (!expected.equals(actual)) {
            throw new AssertionError(what + ": expected " + expected + ", got " + actual);
        }
    }
}
