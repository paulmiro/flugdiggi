use crate::prelude::*;
use flugdiggi_core::config::Curve;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::*;

use super::curve::{CURVE_KINDS, chart};
use crate::app::FlugApp;
use crate::model::SlotData;
use crate::widgets::{byte, card, columns, edited_dot, percent, segmented, slider_row};

/// Raw maximum of trigger curve values.
const TRIGGER_MAX: u8 = 255;

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

fn trigger(d: &mut SlotData, side: Side) -> &mut Curve {
    match side {
        Side::Left => &mut d.config.left_trigger,
        Side::Right => &mut d.config.right_trigger,
    }
}

fn edit_trigger(app: &mut FlugApp, side: Side, f: impl Fn(&mut Curve)) {
    let linked = app.link_triggers;
    app.edit_silent(|d| {
        f(trigger(d, side));
        if linked {
            f(trigger(
                d,
                match side {
                    Side::Left => Side::Right,
                    Side::Right => Side::Left,
                },
            ));
        }
    });
}

impl FlugApp {
    fn trigger_card(&mut self, d: &SlotData, side: Side, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (c, saved) = match side {
            Side::Left => (&d.config.left_trigger, &d.saved_config.left_trigger),
            Side::Right => (&d.config.right_trigger, &d.saved_config.right_trigger),
        };
        let n = match side {
            Side::Left => "lt",
            Side::Right => "rt",
        };
        let id = match side {
            Side::Left => 0usize,
            Side::Right => 1,
        };
        let mut slider = |app: &mut FlugApp, key: &str, value: u8, f: fn(&mut Curve, u8)| {
            app.slider(
                format!("{n}-{key}"),
                (0.0, 255.0),
                f32::from(value),
                move |a, v| {
                    edit_trigger(a, side, |c| f(c, byte(v)));
                },
                window,
                cx,
            )
        };
        let start = slider(self, "start", c.start, |c, v| c.start = v);
        let end = slider(self, "end", c.end, |c, v| c.end = v);
        let p1x = slider(self, "p1x", c.point1[0], |c, v| c.point1[0] = v);
        let p1y = slider(self, "p1y", c.point1[1], |c, v| c.point1[1] = v);
        let p2x = slider(self, "p2x", c.point2[0], |c, v| c.point2[0] = v);
        let p2y = slider(self, "p2y", c.point2[1], |c, v| c.point2[1] = v);
        let edited = c != saved;
        let kind = usize::from(c.kind).min(3);
        let saved_curve = saved.clone();
        let title = match side {
            Side::Left => "Left trigger",
            Side::Right => "Right trigger",
        };
        card(cx)
            .child(
                h_flex()
                    .px_3()
                    .pt_2p5()
                    .gap_2()
                    .child(div().text_sm().font_bold().child(title))
                    .when(edited, |h| h.child(edited_dot()))
                    .child(div().flex_1())
                    .child(
                        Button::new(("reset-trigger", id))
                            .ghost()
                            .small()
                            .icon(Icon::new(gpui_kit::assets::IconName::RotateCcw))
                            .tooltip("Undo changes to this trigger")
                            .disabled(!edited)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                let saved = saved_curve.clone();
                                app.edit(cx, |d| *trigger(d, side) = saved);
                            })),
                    ),
            )
            .child(div().p_3().child(chart(c, TRIGGER_MAX, edited)))
            .child(
                h_flex()
                    .px_3()
                    .pb_2()
                    .justify_between()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Released")
                    .child("Fully pressed"),
            )
            .child(
                div().px_3().pb_3().child(
                    segmented(("trigger-kind", id), &CURVE_KINDS, Some(kind), {
                        let this = cx.entity().downgrade();
                        move |k, _, cx| {
                            this.update(cx, |a, cx| {
                                edit_trigger(a, side, |c| c.kind = u8::try_from(k).unwrap_or(0));
                                cx.notify();
                            })
                            .ok();
                        }
                    })
                    .w_full(),
                ),
            )
            .child(slider_row("Start", &start, percent(c.start, TRIGGER_MAX), false, cx))
            .child(slider_row("End", &end, percent(c.end, TRIGGER_MAX), false, cx))
            .when(kind == 3, |card| {
                card.child(slider_row("Point 1 input", &p1x, percent(c.point1[0], TRIGGER_MAX), false, cx))
                    .child(slider_row("Point 1 output", &p1y, percent(c.point1[1], TRIGGER_MAX), false, cx))
                    .child(slider_row("Point 2 input", &p2x, percent(c.point2[0], TRIGGER_MAX), false, cx))
                    .child(slider_row("Point 2 output", &p2y, percent(c.point2[1], TRIGGER_MAX), false, cx))
            })
            .into_any_element()
    }

    pub fn render_triggers(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let left = self.trigger_card(&d, Side::Left, window, cx);
        let right = self.trigger_card(&d, Side::Right, window, cx);
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .text_sm()
                    .child(
                        div()
                            .flex_1()
                            .text_color(cx.theme().muted_foreground)
                            .child("Start and end set how much of the trigger's travel is ignored at each end."),
                    )
                    .child(Switch::new("link-triggers").label("Link triggers").checked(self.link_triggers).on_click(
                        cx.listener(|a, v: &bool, _, cx| {
                            a.link_triggers = *v;
                            cx.notify();
                        }),
                    )),
            )
            .child(columns(left, right))
            .into_any_element()
    }
}
