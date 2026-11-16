use crate::prelude::*;
use gpui_kit::component::button::Button;
use gpui_kit::component::collapsible::Collapsible as CollapsibleView;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{Sizable, v_flex};
use gpui_kit::*;

use crate::app::FlugApp;
use crate::model::{BUTTONS, key_label};
use crate::widgets::{byte, card, columns, heading, note, row, segmented, slider_row, subheading};

impl FlugApp {
    fn key_menu(
        id: &'static str,
        current: &str,
        set: fn(&mut flugdiggi_core::config::Motion, String),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let this = cx.entity().downgrade();
        let current = current.to_owned();
        Button::new(id)
            .small()
            .w(px(130.))
            .label(key_label(&current))
            .dropdown_caret(true)
            .dropdown_menu(move |menu, _, _| {
                BUTTONS.iter().fold(menu.max_h(px(400.)).scrollable(true), |menu, &b| {
                    let this = this.clone();
                    menu.item(PopupMenuItem::new(key_label(b)).checked(current == b).on_click(move |_, _, cx| {
                        this.update(cx, |a, cx| a.edit(cx, |d| set(&mut d.config.motion, b.to_owned()))).ok();
                    }))
                })
            })
            .into_any_element()
    }

    pub fn render_motion(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let m = &d.config.motion;
        let deadzone = self.slider(
            "motion-dz",
            (0.0, 100.0),
            f32::from(m.deadzone),
            |a, v| {
                a.edit_silent(|d| d.config.motion.deadzone = byte(v));
            },
            window,
            cx,
        );
        let sens_x = self.slider(
            "motion-sx",
            (0.0, 100.0),
            f32::from(m.sensitivity_x),
            |a, v| {
                a.edit_silent(|d| d.config.motion.sensitivity_x = byte(v));
            },
            window,
            cx,
        );
        let sens_y = self.slider(
            "motion-sy",
            (0.0, 100.0),
            f32::from(m.sensitivity_y),
            |a, v| {
                a.edit_silent(|d| d.config.motion.sensitivity_y = byte(v));
            },
            window,
            cx,
        );
        let mut smooth = Vec::new();
        for (i, v) in m.smoothness.iter().enumerate() {
            smooth.push(self.slider(
                format!("motion-smooth{i}"),
                (0.0, 127.0),
                f32::from(*v),
                move |a, v| {
                    a.edit_silent(|d| {
                        if let Some(s) = d.config.motion.smoothness.get_mut(i) {
                            *s = byte(v);
                        }
                    });
                },
                window,
                cx,
            ));
        }
        let off = m.map_to == 0;
        let this = cx.entity().downgrade();
        let pick = |field: fn(&mut flugdiggi_core::config::Motion, u8)| {
            let this = this.clone();
            move |k: usize, _: &mut Window, cx: &mut App| {
                this.update(cx, |a, cx| a.edit(cx, |d| field(&mut d.config.motion, u8::try_from(k).unwrap_or(0)))).ok();
            }
        };
        let advanced_open = self.advanced_open("motion");
        let enable_menu = Self::key_menu("motion-key", &m.enable_key, |m, k| m.enable_key = k, cx);
        let second_menu = Self::key_menu("motion-key2", &m.enable_key2, |m, k| m.enable_key2 = k, cx);
        let advanced_toggle = self.advanced_toggle("motion".to_owned(), cx).into_any_element();
        let smooth_names =
            ["Smoothing start", "Point 1 input", "Point 1 output", "Point 2 input", "Point 2 output", "Smoothing end"];

        let left = v_flex().gap_2().child(heading("Gyro aiming")).child(
            card(cx)
                .child(
                    row(
                        "Act as",
                        None,
                        segmented(
                            "motion-map",
                            &["Off", "Left stick", "Right stick", "Mouse"],
                            Some(usize::from(m.map_to.min(3))),
                            pick(|m, v| m.map_to = v),
                        ),
                        cx,
                    )
                    .border_t_0(),
                )
                .child(row("Turn on with", None, enable_menu, cx).when(off, |r| r.opacity(0.4)))
                .child(
                    row(
                        "Button mode",
                        None,
                        segmented(
                            "motion-mode",
                            &["Toggle", "Hold"],
                            Some(usize::from(m.enable_mode.min(1))),
                            pick(|m, v| m.enable_mode = v),
                        ),
                        cx,
                    )
                    .when(off, |r| r.opacity(0.4)),
                )
                .child(
                    row(
                        "Use",
                        Some(
                            "Shooting: turning the controller moves the camera (with the right stick or mouse). \
                             Racing: tilting it steers like a wheel (with the left stick).",
                        ),
                        segmented(
                            "motion-use",
                            &["Shooting", "Racing"],
                            Some(usize::from(m.use_mode.min(1))),
                            pick(|m, v| m.use_mode = v),
                        ),
                        cx,
                    )
                    .when(off, |r| r.opacity(0.4)),
                ),
        );
        let right = v_flex()
            .gap_2()
            .child(heading("Response"))
            .child(
                card(cx)
                    .child(slider_row("Deadzone", &deadzone, m.deadzone.to_string(), off, cx).border_t_0())
                    .child(slider_row("Horizontal sensitivity", &sens_x, m.sensitivity_x.to_string(), off, cx))
                    .child(slider_row("Vertical sensitivity", &sens_y, m.sensitivity_y.to_string(), off, cx))
                    .child(advanced_toggle)
                    .child(
                        CollapsibleView::new().open(advanced_open).motion_id("adv-motion").content(
                            v_flex()
                                .child(subheading("Values the official app does not show", cx))
                                .child(row("Second button", None, second_menu, cx))
                                .children(smooth.iter().enumerate().map(|(i, s)| {
                                    let v = m.smoothness.get(i).copied().unwrap_or(0);
                                    slider_row(
                                        smooth_names.get(i).copied().unwrap_or("Smoothing"),
                                        s,
                                        v.to_string(),
                                        false,
                                        cx,
                                    )
                                })),
                        ),
                    ),
            )
            .child(note("Gyro values are the controller's raw units.", cx));
        columns(left, right, self.narrow).into_any_element()
    }
}
