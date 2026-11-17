use crate::prelude::*;
use flugdiggi_core::config::RumbleMotor;
use gpui_kit::component::collapsible::Collapsible as CollapsibleView;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::v_flex;
use gpui_kit::*;

use crate::app::FlugApp;
use crate::model::SlotData;
use crate::widgets::{byte, card, columns, heading, note, row, slider_row, subheading};

fn motor(d: &mut SlotData, left: bool) -> &mut RumbleMotor {
    if left { &mut d.config.rumble.left } else { &mut d.config.rumble.right }
}

impl FlugApp {
    fn motor_rows(&mut self, d: &SlotData, left: bool, window: &mut Window, cx: &mut Context<Self>) -> (Div, Div) {
        let m = if left { &d.config.rumble.left } else { &d.config.rumble.right };
        let k = if left { "l" } else { "r" };
        let off = !d.config.rumble.enabled;
        let strength = self.slider(
            format!("rumble-{k}-scale"),
            (0.0, 100.0),
            f32::from(m.scale),
            move |a, v| {
                a.edit_silent(|d| motor(d, left).scale = byte(v));
            },
            window,
            cx,
        );
        let min = self.slider(
            format!("rumble-{k}-min"),
            (0.0, 255.0),
            f32::from(m.min),
            move |a, v| {
                a.edit_silent(|d| motor(d, left).min = byte(v));
            },
            window,
            cx,
        );
        let max = self.slider(
            format!("rumble-{k}-max"),
            (0.0, 255.0),
            f32::from(m.max),
            move |a, v| {
                a.edit_silent(|d| motor(d, left).max = byte(v));
            },
            window,
            cx,
        );
        let kind = self.slider(
            format!("rumble-{k}-kind"),
            (0.0, 255.0),
            f32::from(m.kind),
            move |a, v| {
                a.edit_silent(|d| motor(d, left).kind = byte(v));
            },
            window,
            cx,
        );
        let title = if left { "Left motor (strong)" } else { "Right motor (weak)" };
        let basic = v_flex()
            .child(subheading(title, cx))
            .child(slider_row("Strength", &strength, format!("{}%", m.scale), off, cx).border_t_0());
        let advanced = v_flex()
            .child(subheading(title, cx))
            .child(slider_row("Minimum output", &min, m.min.to_string(), off, cx).border_t_0())
            .child(slider_row("Maximum output", &max, m.max.to_string(), off, cx))
            .child(slider_row("Motor type", &kind, m.kind.to_string(), off, cx));
        (basic, advanced)
    }

    pub fn render_vibration(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let on = d.config.rumble.enabled;
        let (left, left_adv) = self.motor_rows(&d, true, window, cx);
        let (right, right_adv) = self.motor_rows(&d, false, window, cx);
        let advanced_open = self.advanced_open("rumble");
        let rumble = v_flex().gap_2().child(heading("Rumble")).child(
            card(cx)
                .child(
                    row(
                        "Rumble",
                        None,
                        Switch::new("rumble-on").checked(on).on_click(cx.listener(|a, v: &bool, _, cx| {
                            let v = *v;
                            a.edit(cx, |d| d.config.rumble.enabled = v);
                        })),
                        cx,
                    )
                    .border_t_0(),
                )
                .child(div().when(!on, |d| d.opacity(0.4)).child(left).child(right))
                .child(self.advanced_toggle("rumble".to_owned(), cx))
                .child(
                    CollapsibleView::new().open(advanced_open).motion_id("adv-rumble").content(
                        v_flex()
                            .when(!on, |d| d.opacity(0.4))
                            .child(subheading("Values the official app does not show", cx))
                            .child(left_adv)
                            .child(right_adv),
                    ),
                ),
        );
        let info = v_flex()
            .gap_2()
            .child(heading("Trigger vibration"))
            .child(note("The Vader 4 Pro has no motors in its triggers, so there is nothing to set here.", cx));
        columns(rumble, info, self.narrow).into_any_element()
    }
}
