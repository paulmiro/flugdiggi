use crate::prelude::*;
use flugdiggi_core::config::Curve;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::*;

use super::curve::{self, TRIGGER_MAX, chart};
use crate::app::FlugApp;
use crate::model::{Side, SlotData};
use crate::widgets::{byte, card, columns, edited_dot, percent, slider_row_sub};

fn trigger(d: &mut SlotData, side: Side) -> &mut Curve {
    match side {
        Side::Left => &mut d.config.left_trigger,
        Side::Right => &mut d.config.right_trigger,
    }
}

fn name(side: Side) -> &'static str {
    match side {
        Side::Left => "left",
        Side::Right => "right",
    }
}

/// Applies `f` to the trigger, and copies the result to the other trigger while it is being copied there.
fn edit_trigger(app: &mut FlugApp, side: Side, f: impl FnOnce(&mut Curve)) {
    let copy = app.trigger_copy == Some(side);
    app.edit_silent(|d| {
        f(trigger(d, side));
        if copy {
            let c = trigger(d, side).clone();
            *trigger(d, side.other()) = c;
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
        let mut slider = |app: &mut FlugApp, key: &str, value: u8, f: fn(&mut Curve, u8)| {
            app.slider(
                format!("{n}-{key}"),
                (0.0, f32::from(TRIGGER_MAX)),
                f32::from(value),
                move |a, v| edit_trigger(a, side, |c| f(c, byte(v))),
                window,
                cx,
            )
        };
        let start = slider(self, "start", c.start, curve::set_trigger_start);
        let end = slider(self, "end", c.end, curve::set_trigger_end);
        let edited = c != saved;
        let copying = self.trigger_copy == Some(side);
        let copied_here = self.trigger_copy == Some(side.other());
        let busy = self.is_busy();
        let saved_curve = saved.clone();
        let title = match side {
            Side::Left => "Left trigger",
            Side::Right => "Right trigger",
        };
        let header = h_flex()
            .px_3()
            .pt_2p5()
            .gap_2()
            .child(div().text_sm().font_bold().child(title))
            .when(edited, |h| h.child(edited_dot()))
            .child(div().flex_1())
            .child(
                Switch::new(("copy-trigger", side.index()))
                    .small()
                    .label(format!("Copy to {} trigger", name(side.other())))
                    .checked(copying)
                    .disabled(busy)
                    .on_click(cx.listener(move |a, on: &bool, _, cx| {
                        if *on {
                            a.trigger_copy = Some(side);
                            a.edit(cx, |d| {
                                let c = trigger(d, side).clone();
                                *trigger(d, side.other()) = c;
                            });
                        } else {
                            a.trigger_copy = None;
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new(("reset-trigger", side.index()))
                    .ghost()
                    .small()
                    .icon(Icon::new(gpui_kit::assets::IconName::RotateCcw))
                    .tooltip("Undo changes to this trigger")
                    .disabled(!edited || copied_here)
                    .on_click(cx.listener(move |a, _, _, cx| {
                        let saved = saved_curve.clone();
                        edit_trigger(a, side, |c| *c = saved);
                        cx.notify();
                    })),
            );
        let body = v_flex()
            .relative()
            .when(copied_here, |b| b.opacity(0.4))
            .child(div().p_3().child(chart(curve::trigger_points(c), curve::trigger_active(c), edited)))
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
            .child(slider_row_sub(
                "Start",
                Some("Travel at the start that is ignored"),
                &start,
                percent(c.start, TRIGGER_MAX),
                false,
                cx,
            ))
            .child(slider_row_sub(
                "End",
                Some("Pressing this far already counts as fully pressed"),
                &end,
                percent(c.end, TRIGGER_MAX),
                false,
                cx,
            ))
            .when(copied_here, |b| b.child(div().absolute().top_0().left_0().size_full().occlude()));
        card(cx).child(header).child(body).into_any_element()
    }

    pub fn render_triggers(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let left = self.trigger_card(&d, Side::Left, window, cx);
        let right = self.trigger_card(&d, Side::Right, window, cx);
        columns(left, right, self.narrow).into_any_element()
    }
}
