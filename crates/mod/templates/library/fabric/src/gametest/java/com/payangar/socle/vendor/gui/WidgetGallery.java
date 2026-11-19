package com.payangar.socle.vendor.gui;

import com.payangar.socle.vendor.gui.screen.ConfigScreen;
import com.payangar.socle.vendor.gui.widget.Binding;
import com.payangar.socle.vendor.gui.widget.ButtonWidget;
import com.payangar.socle.vendor.gui.widget.CheckboxWidget;
import com.payangar.socle.vendor.gui.widget.ColourPicker;
import com.payangar.socle.vendor.gui.widget.ColourSwatch;
import com.payangar.socle.vendor.gui.widget.Cycle;
import com.payangar.socle.vendor.gui.widget.Dropdown;
import com.payangar.socle.vendor.gui.widget.Heading;
import com.payangar.socle.vendor.gui.widget.Label;
import com.payangar.socle.vendor.gui.widget.ResetButton;
import com.payangar.socle.vendor.gui.widget.Segmented;
import com.payangar.socle.vendor.gui.widget.Slider;
import com.payangar.socle.vendor.gui.widget.Tabs;
import com.payangar.socle.vendor.gui.widget.TextField;
import com.payangar.socle.vendor.gui.widget.Toggle;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.network.chat.Component;

/**
 * A custom screen built on {@link ConfigScreen} with every widget of the kit, once enabled and
 * once disabled, and a colour picker: what the widget state screenshots show.
 */
final class WidgetGallery extends ConfigScreen {
    static final int COLUMN = 150;

    final Binding<Boolean> toggle = Binding.holding(true);
    final Binding<Boolean> check = Binding.holding(true);
    final Binding<Integer> number = Binding.holding(7);
    final Binding<String> segment = Binding.holding("Two");
    final Binding<String> choice = Binding.holding("Beta");
    final Binding<String> cycle = Binding.holding("North");
    final Binding<String> text = Binding.holding("Some text");
    final Binding<Integer> colour = Binding.holding(0xC03F7FE8);
    final Binding<Integer> tab = Binding.holding(0);
    /** The enabled button, which the test hovers. */
    ButtonWidget button;
    /** The enabled dropdown, which the test opens. */
    Dropdown<String> dropdown;

    WidgetGallery() {
        super(Component.literal("Widget gallery"), null);
    }

    @Override
    protected void initContent(int x, int y, int width, int height) {
        column(x, y, true);
        column(x + COLUMN + 2 * Theme.PADDING, y, false);
        int third = x + 2 * (COLUMN + 2 * Theme.PADDING);
        Tabs tabs = addRenderableWidget(new Tabs(
                Component.literal("Tabs"),
                List.of(Component.literal("First"), Component.literal("Second"), Component.literal("Third")),
                tab));
        tabs.setPosition(third, y);
        ColourPicker picker = addRenderableWidget(new ColourPicker(Component.literal("Picker"), colour, true));
        picker.setPosition(third, y + tabs.getHeight() + Theme.PADDING);
        TextField search = addRenderableWidget(new TextField(COLUMN, Component.literal("Search"), Binding.holding(""))
                .hint(Component.literal("Search...")));
        search.setPosition(third, picker.getBottom() + Theme.PADDING);
    }

    private void column(int x, int y, boolean enabled) {
        List<AbstractWidget> widgets = new ArrayList<>();
        widgets.add(new Heading(COLUMN, Component.literal(enabled ? "Enabled" : "Disabled")));
        widgets.add(new Label(COLUMN, Component.literal("A label")));
        ButtonWidget pressable = new ButtonWidget(COLUMN, Component.literal("Button"), b -> {});
        widgets.add(pressable);
        ResetButton reset = new ResetButton(Component.literal("Reset"), number, 3);
        reset.setEnabled(enabled);
        widgets.add(reset);
        widgets.add(new Toggle(Component.literal("Toggle"), toggle));
        widgets.add(new CheckboxWidget(COLUMN, Component.literal("Checkbox"), check));
        widgets.add(Slider.integer(COLUMN, Component.literal("Slider"), 0, 10, number));
        widgets.add(new Segmented<>(COLUMN, Component.literal("Segmented"), List.of("One", "Two", "Three"), Component::literal, segment));
        Dropdown<String> list = new Dropdown<>(
                COLUMN, Component.literal("Dropdown"), List.of("Alpha", "Beta", "Gamma", "Delta"), Component::literal, choice);
        widgets.add(list);
        widgets.add(new Cycle<>(COLUMN, Component.literal("Cycle"), List.of("North", "East", "South", "West"), Component::literal, cycle));
        widgets.add(new TextField(COLUMN, Component.literal("Text"), text));
        widgets.add(new ColourSwatch(COLUMN, Component.literal("Colour"), colour, true));
        int cy = y;
        for (AbstractWidget widget : widgets) {
            if (widget != reset) {
                widget.active = enabled;
            }
            widget.setPosition(x, cy);
            cy += widget.getHeight() + 4;
            addRenderableWidget(widget);
        }
        if (enabled) {
            button = pressable;
            dropdown = list;
        }
    }
}
