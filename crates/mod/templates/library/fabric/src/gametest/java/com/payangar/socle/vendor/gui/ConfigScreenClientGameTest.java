package com.payangar.socle.vendor.gui;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.platform.InputConstants;
import com.payangar.socle.vendor.config.ClientSettings;
import com.payangar.socle.vendor.config.ConfigSchema;
import com.payangar.socle.vendor.config.ConfigValues;
import com.payangar.socle.vendor.config.ModSide;
import com.payangar.socle.vendor.config.Scope;
import com.payangar.socle.vendor.config.Setting;
import com.payangar.socle.vendor.config.SettingsSource;
import com.payangar.socle.vendor.gui.screen.GeneratedConfigScreen;
import com.payangar.socle.vendor.gui.screen.SettingRow;
import com.payangar.socle.vendor.gui.screen.SettingWidgets;
import com.payangar.socle.vendor.gui.widget.Binding;
import com.payangar.socle.vendor.gui.widget.ColourPicker;
import com.payangar.socle.vendor.gui.widget.Dropdown;
import com.payangar.socle.vendor.gui.widget.Slider;
import com.payangar.socle.vendor.gui.widget.TextField;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.PosixFilePermissions;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;
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
 * states compared with their reference renders, a setting changed with the mouse and the keyboard
 * reaching the JSON file, and cases that once broke. References are renders of CI, like every
 * screenshot test.
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
        assertRender(context, "vendor_config_screen");

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
        assertRender(context, "vendor_widget_states");

        // An open dropdown above the widgets under it.
        AbstractWidget dropdown = context.computeOnClient(mc -> ((WidgetGallery) mc.gui.screen()).dropdown);
        click(context, dropdown, 0.5, 0.5);
        context.getInput().setCursorPos(1, 1);
        context.waitTicks(SETTLE_TICKS);
        assertRender(context, "vendor_dropdown_open");

        // Cases that once broke, each checked on its own so that one failing hides no other.
        Path dir = file.getParent();
        List<String> failures = new ArrayList<>();
        ConfigSchema.Builder numbers = ConfigSchema.builder(modId, ModSide.BOTH_REQUIRED);
        List<Setting<?>> typed = List.of(
                numbers.define(Setting.integer("slots", Scope.CLIENT, 4, 4, 4)),
                numbers.define(Setting.decimal("distance", Scope.CLIENT, 128, 0, 1e6)),
                numbers.define(Setting.decimal("anything", Scope.CLIENT, 0, -Double.MAX_VALUE, Double.MAX_VALUE)));
        ConfigValues numberValues = new ConfigValues(numbers.build(), Scope.CLIENT);
        for (Setting<?> setting : typed) {
            check(failures, "the widget of " + setting.name(), () -> typedField(context, numberValues, setting));
        }
        check(failures, "an unreadable client file", () -> unreadableFile(schema, volume, dir.resolve("unreadable.json")));
        check(failures, "a typed field after a reset", () -> resetTypedField(context, modId, dir.resolve("typed.json")));
        check(failures, "a text longer than its field holds", () -> longText(context, modId, dir.resolve("long.json")));
        check(failures, "a dropdown driven by the keyboard", () -> dropdownKeys(context));
        check(failures, "a drag in the colour square", () -> colourDrag(context));
        if (!failures.isEmpty()) {
            throw new AssertionError(failures.size() + " case(s) failed:\n" + String.join("\n", failures));
        }

        context.restoreDefaultGameOptions();
        context.runOnClient(Minecraft::resizeGui);
        context.setScreen(() -> null);
    }

    /**
     * A number setting whose bounds leave nothing to slide, or too wide a range for a slider's
     * hundred steps, gets a typed field.
     */
    private static void typedField(ClientGameTestContext context, ConfigValues values, Setting<?> setting) {
        AbstractWidget widget = context.computeOnClient(mc -> SettingWidgets.create(SettingRow.CONTROL_WIDTH, setting, Binding.of(values, setting)));
        assertEquals(TextField.class, widget.getClass(), setting.name() + "'s widget");
    }

    /** Settings that could not be read from their file are never saved over it. */
    private static void unreadableFile(ConfigSchema schema, Setting<Integer> volume, Path file) throws IOException {
        String text = "{\"volume\": 12}\n";
        Files.writeString(file, text);
        Files.setPosixFilePermissions(file, Set.of());
        try {
            assertEquals(false, Files.isReadable(file), file + " readable without permissions (run as root?)");
            ClientSettings settings = ClientSettings.at(schema, file);
            settings.values().set(volume, 30);
            try {
                settings.save();
                throw new AssertionError("the defaults were saved over " + file);
            } catch (IOException refused) {
                // Expected: the file is left for the player to fix.
            }
        } finally {
            Files.setPosixFilePermissions(file, PosixFilePermissions.fromString("rw-------"));
        }
        assertEquals(text, Files.readString(file), file + " after a save");
    }

    /** A typed field showing text it refused shows the value again once reset. */
    private static void resetTypedField(ClientGameTestContext context, String modId, Path file) {
        ConfigSchema.Builder builder = ConfigSchema.builder(modId, ModSide.BOTH_REQUIRED);
        Setting<Integer> wide = builder.define(Setting.integer("wide", Scope.CLIENT, 5, 0, 100_000));
        ConfigSchema schema = builder.build();
        delete(file);
        ClientSettings client = ClientSettings.at(schema, file);
        context.setScreen(() -> new GeneratedConfigScreen(Component.literal("Typed"), null, schema, Map.of(Scope.CLIENT, client)));
        context.waitTick();
        SettingRow row = row(context, wide);
        click(context, row.control(), 0.95, 0.5);
        context.getInput().typeChars("42a");
        context.waitTick();
        click(context, row.reset(), 0.5, 0.5);
        context.waitTick();
        assertEquals("5", context.computeOnClient(mc -> ((TextField) row.control()).text()), "the field after a reset");
    }

    /**
     * A text longer than a field holds, written in the file by hand, is shown whole: deleting a
     * character keeps the rest of it.
     */
    private static void longText(ClientGameTestContext context, String modId, Path file) throws IOException {
        ConfigSchema.Builder builder = ConfigSchema.builder(modId, ModSide.BOTH_REQUIRED);
        Setting<String> motd = builder.define(Setting.string("motd", Scope.CLIENT, ""));
        ConfigSchema schema = builder.build();
        Files.writeString(file, "{\"motd\": \"" + "x".repeat(2000) + "\"}");
        ClientSettings client = ClientSettings.at(schema, file);
        context.setScreen(() -> new GeneratedConfigScreen(Component.literal("Long"), null, schema, Map.of(Scope.CLIENT, client)));
        context.waitTick();
        click(context, row(context, motd).control(), 0.5, 0.5);
        context.getInput().pressKey(InputConstants.KEY_END);
        context.getInput().pressKey(InputConstants.KEY_BACKSPACE);
        context.waitTick();
        assertEquals(1999, context.computeOnClient(mc -> client.values().get(motd).length()), "the text's length after a deletion");
    }

    /** Arrow keys move the highlight of an open list under a mouse that does not move. */
    private static void dropdownKeys(ClientGameTestContext context) {
        context.setScreen(WidgetGallery::new);
        context.waitTick();
        WidgetGallery gallery = context.computeOnClient(mc -> (WidgetGallery) mc.gui.screen());
        click(context, context.computeOnClient(mc -> gallery.dropdown), 0.5, 0.5);
        AbstractWidget list = context.computeOnClient(mc -> gallery.popup());
        // The mouse rests on the first choice; two presses of Down then Enter pick the third.
        moveTo(context, list.getX() + 10, list.getY() + 1 + Dropdown.ITEM_HEIGHT / 2.0);
        context.waitTick();
        for (int key : new int[] {InputConstants.KEY_DOWN, InputConstants.KEY_DOWN, InputConstants.KEY_RETURN}) {
            context.getInput().pressKey(key);
            context.waitTick();
        }
        assertEquals("Gamma", context.computeOnClient(mc -> gallery.choice.get()), "the choice picked with the keyboard");
    }

    /**
     * A drag through the nearly grey left edge of the square and back keeps the hue picked: the
     * colour back at the first point is the colour first picked there.
     */
    private static void colourDrag(ClientGameTestContext context) {
        context.setScreen(WidgetGallery::new);
        context.waitTick();
        WidgetGallery gallery = context.computeOnClient(mc -> (WidgetGallery) mc.gui.screen());
        ColourPicker picker = context.computeOnClient(mc -> gallery.picker);
        // The square is 64 pixels wide, the hue bar right of it after a gap of 4.
        double left = picker.getX() + Theme.PADDING;
        double top = picker.getY() + Theme.PADDING;
        clickAt(context, left + 64 + 4 + 4, top + 5);
        moveTo(context, left + 56, top + 6);
        context.getInput().holdMouse(InputConstants.MOUSE_BUTTON_LEFT);
        try {
            context.waitTick();
            int picked = context.computeOnClient(mc -> gallery.colour.get());
            moveTo(context, left + 1, top + 6);
            context.waitTick();
            moveTo(context, left + 56, top + 6);
            context.waitTick();
            assertEquals(
                    ColourPicker.format(picked, true),
                    ColourPicker.format(context.computeOnClient(mc -> gallery.colour.get()), true),
                    "the colour back at the point first picked");
        } finally {
            context.getInput().releaseMouse(InputConstants.MOUSE_BUTTON_LEFT);
        }
    }

    private interface Check {
        void run() throws Exception;
    }

    /** Runs {@code check}, adding {@code what} and its failure to {@code failures}. */
    private static void check(List<String> failures, String what, Check check) {
        try {
            check.run();
        } catch (Throwable e) {
            failures.add(what + ": " + e);
        }
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
     * Compares the screen with the reference {@code name}, except under a test profile
     * ({@code mod.testProfile}), whose mods change the render.
     */
    private static void assertRender(ClientGameTestContext context, String name) {
        if (System.getProperty("mod.testProfile") == null) {
            context.assertScreenshotEquals(reference(name));
        }
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
        return row(context, setting).control();
    }

    /** The row of {@code setting} in the generated screen shown. */
    private static SettingRow row(ClientGameTestContext context, Setting<?> setting) {
        return context.computeOnClient(mc -> ((GeneratedConfigScreen) mc.gui.screen()).rows().stream()
                .filter(row -> row.setting() == setting)
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
