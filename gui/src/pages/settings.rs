use crate::prelude::*;
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme, Sizable, v_flex};
use gpui_kit::*;

use crate::app::FlugApp;
use crate::model::display_name;
use crate::widgets::{card, columns, heading, row};

/// Longest name the controller stores, in UTF-16 units.
const NAME_MAX: usize = 8;

impl FlugApp {
    pub fn render_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let Some(loaded) = self.loaded().cloned() else { return div().into_any_element() };
        let name = self.input(
            "slot-name",
            display_name(&d.config.name),
            |a, text| {
                let name: String = text
                    .chars()
                    .scan(0usize, |n, c| {
                        *n = n.saturating_add(c.len_utf16());
                        (*n <= NAME_MAX).then_some(c)
                    })
                    .collect();
                a.edit_silent(|d| d.config.name = name);
            },
            window,
            cx,
        );
        let slot = self.slot;
        let is_active = usize::from(loaded.active.slot.get()) == slot;
        let switch_mode = slot >= 4;
        let busy = self.is_busy();
        let (active_label, active_sub) = match (switch_mode, is_active) {
            (true, _) => ("Switch mode slot", "Used when the controller is in Switch mode"),
            (false, true) => ("Active slot", "The controller is using this slot now"),
            (false, false) => ("Active slot", "Make the controller use this slot"),
        };
        let mono = cx.theme().mono_font_family.clone();
        let muted = cx.theme().muted_foreground;
        let info_row = |label: &str, value: String, mono_font: bool, cx: &App| {
            row(
                label.to_owned(),
                None,
                div().text_color(muted).when(mono_font, |d| d.font_family(mono.clone())).child(value),
                cx,
            )
        };
        let i = &loaded.info;
        let entry = self.entry();
        let product = entry.map_or_else(String::new, |e| e.summary.product.clone());
        let port = entry.map_or_else(String::new, |e| e.summary.port.to_string());

        let left = v_flex()
            .gap_2()
            .child(heading(format!("Slot {slot}")))
            .child(
                card(cx)
                    .child(
                        row("Name", Some("Up to 8 characters"), div().w(px(160.)).child(Input::new(&name).small()), cx)
                            .border_t_0(),
                    )
                    .child(row(
                        active_label,
                        Some(active_sub),
                        Button::new("activate")
                            .small()
                            .label(if is_active { "Active" } else { "Activate" })
                            .disabled(is_active || switch_mode || busy)
                            .on_click(cx.listener(|a, _, window, cx| a.request_activate(a.slot, window, cx))),
                        cx,
                    )),
            )
            .child(heading("Files").mt_2())
            .child(
                card(cx)
                    .child(
                        row(
                            "Export this slot as TOML",
                            None,
                            Button::new("export")
                                .small()
                                .label("Export…")
                                .on_click(cx.listener(|a, _, _, cx| a.export(cx))),
                            cx,
                        )
                        .border_t_0(),
                    )
                    .child(row(
                        "Load a TOML file into this slot",
                        Some("Shows up as unsaved changes until you save"),
                        Button::new("import")
                            .small()
                            .label("Import…")
                            .disabled(busy)
                            .on_click(cx.listener(|_, _, _, cx| FlugApp::import(cx))),
                        cx,
                    ))
                    .child(row(
                        "Backups",
                        Some("A copy of the slot is kept before every save"),
                        Button::new("backups")
                            .small()
                            .ghost()
                            .label("Open folder")
                            .on_click(cx.listener(|_, _, _, cx| FlugApp::open_backups(cx))),
                        cx,
                    )),
            );
        let right = v_flex().gap_2().child(heading("Controller")).child(
            card(cx)
                .child(info_row("Model", product, false, cx).border_t_0())
                .child(info_row("Connection", i.connection.to_string(), false, cx))
                .child(info_row("USB port", port, true, cx))
                .child(info_row("MAC", i.mac.to_string(), true, cx))
                .child(info_row("Firmware", i.firmware.to_string(), true, cx))
                .child(info_row("Battery", i.battery.to_string(), false, cx))
                .child(info_row("Data version", format!("{:04x}", d.saved_config.data_version), true, cx)),
        );
        columns(left, right, self.narrow).into_any_element()
    }

    /// Shown until the controller's data is loaded: only what can be read without detaching xpad.
    pub fn render_unloaded(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(s) = self.entry().map(|e| e.summary.clone()) else { return div().into_any_element() };
        let mono = cx.theme().mono_font_family.clone();
        let muted = cx.theme().muted_foreground;
        let info_row = |label: &str, value: String, mono_font: bool, cx: &App| {
            row(
                label.to_owned(),
                None,
                div().text_color(muted).when(mono_font, |d| d.font_family(mono.clone())).child(value),
                cx,
            )
        };
        let connection =
            if s.connection_guessed { format!("{} (guessed)", s.connection) } else { s.connection.to_string() };
        let left = v_flex()
            .gap_2()
            .child(heading("Controller data"))
            .child(Alert::warning(
                "load-warning",
                "While loading, which takes a few seconds, the controller is disconnected from games. It \
                 reconnects by itself when loading is done.",
            ))
            .child(
                Button::new("load")
                    .primary()
                    .label("Load Controller Data")
                    .disabled(self.is_busy())
                    .on_click(cx.listener(|a, _, _, cx| a.reload(cx))),
            );
        let right = v_flex().gap_2().child(heading("Controller")).child(
            card(cx)
                .child(info_row("Model", s.product, false, cx).border_t_0())
                .child(info_row("Connection", connection, false, cx))
                .child(info_row("USB port", s.port.to_string(), true, cx)),
        );
        columns(left, right, self.narrow).into_any_element()
    }
}
