use crate::prelude::*;
use flugdiggi_core::config::Stick;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::collapsible::Collapsible as CollapsibleView;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::*;

use super::curve::{self, CURVE_CUSTOM, CURVE_KINDS, STICK_MAX, chart};
use crate::app::FlugApp;
use crate::model::{Side, SlotData};
use crate::widgets::{byte, card, columns, edited_dot, percent, row, segmented, slider_row_sub, subheading};

fn stick(d: &mut SlotData, side: Side) -> &mut Stick {
    match side {
        Side::Left => &mut d.config.left_stick,
        Side::Right => &mut d.config.right_stick,
    }
}

fn name(side: Side) -> &'static str {
    match side {
        Side::Left => "left",
        Side::Right => "right",
    }
}

/// The outer deadzone part of the edge byte (values above 127 mean something else, see `curve`).
fn outer(s: &Stick) -> u8 {
    if s.edge <= STICK_MAX { s.edge } else { 0 }
}

/// Applies `f` to the stick, and copies the result to the other stick while it is being copied there.
fn edit_stick(app: &mut FlugApp, side: Side, f: impl FnOnce(&mut Stick)) {
    let copy = app.stick_copy == Some(side);
    app.edit_silent(|d| {
        f(stick(d, side));
        if copy {
            let s = stick(d, side).clone();
            *stick(d, side.other()) = s;
        }
    });
}

/// A slider that edits the stick on `side`.
fn stick_slider(
    app: &mut FlugApp,
    side: Side,
    key: &str,
    value: u8,
    set: fn(&mut Stick, u8),
    window: &mut Window,
    cx: &mut Context<FlugApp>,
) -> Entity<gpui_kit::component::slider::SliderState> {
    app.slider(
        format!("{}-{key}", name(side)),
        (0.0, f32::from(STICK_MAX)),
        f32::from(value),
        move |a, v| edit_stick(a, side, |s| set(s, byte(v))),
        window,
        cx,
    )
}

impl FlugApp {
    fn stick_card(&mut self, d: &SlotData, side: Side, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (s, saved) = match side {
            Side::Left => (&d.config.left_stick, &d.saved_config.left_stick),
            Side::Right => (&d.config.right_stick, &d.saved_config.right_stick),
        };
        let c = s.curve.clone();
        let deadzone =
            stick_slider(self, side, "dz", c.start, |s, v| curve::set_stick_deadzones(s, v, outer(s)), window, cx);
        let outer_dz = stick_slider(
            self,
            side,
            "outer",
            outer(s),
            |s, v| curve::set_stick_deadzones(s, s.curve.start, v),
            window,
            cx,
        );
        let p1x = stick_slider(self, side, "p1x", c.point1[0], |s, v| curve::set_stick_point(s, 0, 0, v), window, cx);
        let p1y = stick_slider(self, side, "p1y", c.point1[1], |s, v| curve::set_stick_point(s, 0, 1, v), window, cx);
        let p2x = stick_slider(self, side, "p2x", c.point2[0], |s, v| curve::set_stick_point(s, 1, 0, v), window, cx);
        let p2y = stick_slider(self, side, "p2y", c.point2[1], |s, v| curve::set_stick_point(s, 1, 1, v), window, cx);
        let end = stick_slider(self, side, "end", c.end, |s, v| s.curve.end = v, window, cx);
        let mut table = Vec::new();
        for (i, v) in s.curve_points.iter().enumerate() {
            table.push(self.slider(
                format!("{}-ext{i}", name(side)),
                (0.0, 255.0),
                f32::from(*v),
                move |a, v| {
                    edit_stick(a, side, |s| {
                        if let Some(p) = s.curve_points.get_mut(i) {
                            *p = byte(v);
                        }
                    });
                },
                window,
                cx,
            ));
        }
        let advanced_open = self.advanced_open(&format!("stick-{}", name(side)));
        let edited = s != saved;
        let copying = self.stick_copy == Some(side);
        let copied_here = self.stick_copy == Some(side.other());
        let busy = self.is_busy();
        let title = match side {
            Side::Left => "Left stick",
            Side::Right => "Right stick",
        };
        let kind = usize::from(c.kind.min(CURVE_CUSTOM));
        let saved_stick = saved.clone();
        let this = cx.entity().downgrade();
        let header = h_flex()
            .px_3()
            .pt_2p5()
            .gap_2()
            .child(div().text_sm().font_bold().child(title))
            .when(edited, |h| h.child(edited_dot()))
            .child(div().flex_1())
            .child(
                Switch::new(("copy-stick", side.index()))
                    .small()
                    .label(format!("Copy to {} stick", name(side.other())))
                    .checked(copying)
                    .disabled(busy)
                    .on_click(cx.listener(move |a, on: &bool, _, cx| {
                        if *on {
                            a.stick_copy = Some(side);
                            a.edit(cx, |d| {
                                let s = stick(d, side).clone();
                                *stick(d, side.other()) = s;
                            });
                        } else {
                            a.stick_copy = None;
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new(("reset-stick", side.index()))
                    .ghost()
                    .small()
                    .icon(Icon::new(gpui_kit::assets::IconName::RotateCcw))
                    .tooltip("Undo changes to this stick")
                    .disabled(!edited || copied_here)
                    .on_click(cx.listener(move |a, _, _, cx| {
                        let saved = saved_stick.clone();
                        edit_stick(a, side, |s| *s = saved);
                        cx.notify();
                    })),
            );
        let table_label = |i: usize| format!("At {}% input", i.saturating_mul(100) / 8);
        let body = v_flex()
            .relative()
            .when(copied_here, |b| b.opacity(0.4))
            .child(div().p_3().child(chart(
                curve::stick_points(s),
                curve::stick_active(s),
                c != saved.curve || s.edge != saved.edge,
            )))
            .child(
                v_flex()
                    .px_3()
                    .pb_3()
                    .gap_1p5()
                    .child(
                        segmented(("stick-kind", side.index()), &CURVE_KINDS, Some(kind), {
                            let this = this.clone();
                            move |k, _, cx| {
                                this.update(cx, |a, cx| {
                                    edit_stick(a, side, |s| curve::set_stick_kind(s, u8::try_from(k).unwrap_or(0)));
                                    cx.notify();
                                })
                                .ok();
                            }
                        })
                        .w_full(),
                    )
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(
                        "Quick reacts strongly to small movements, Slow gives finer control near the center. \
                         Choosing a preset resets both deadzones.",
                    )),
            )
            .child(slider_row_sub(
                "Deadzone",
                Some("Movement around the center that is ignored"),
                &deadzone,
                percent(c.start, STICK_MAX),
                false,
                cx,
            ))
            .child(slider_row_sub(
                "Outer deadzone",
                Some("Pushing this far already counts as all the way"),
                &outer_dz,
                percent(outer(s), STICK_MAX),
                false,
                cx,
            ))
            .when(kind == usize::from(CURVE_CUSTOM), |card| {
                card.child(slider_row_sub(
                    "Point 1 input",
                    Some("How far the stick is pushed at the first bend"),
                    &p1x,
                    percent(c.point1[0], STICK_MAX),
                    false,
                    cx,
                ))
                .child(slider_row_sub(
                    "Point 1 output",
                    Some("How far the game sees it pushed there"),
                    &p1y,
                    percent(c.point1[1], STICK_MAX),
                    false,
                    cx,
                ))
                .child(slider_row_sub(
                    "Point 2 input",
                    Some("How far the stick is pushed at the second bend"),
                    &p2x,
                    percent(c.point2[0], STICK_MAX),
                    false,
                    cx,
                ))
                .child(slider_row_sub(
                    "Point 2 output",
                    Some("How far the game sees it pushed there"),
                    &p2y,
                    percent(c.point2[1], STICK_MAX),
                    false,
                    cx,
                ))
            })
            .child(row(
                "Range shape",
                Some("How far the stick reaches diagonally"),
                segmented(("circ", side.index()), &["Square", "Round"], Some(usize::from(s.circularity.min(1))), {
                    let this = this.clone();
                    move |k, _, cx| {
                        this.update(cx, |a, cx| {
                            edit_stick(a, side, |s| s.circularity = u8::try_from(k).unwrap_or(0));
                            cx.notify();
                        })
                        .ok();
                    }
                }),
                cx,
            ))
            .child(self.advanced_toggle(format!("stick-{}", name(side)), cx))
            .child(
                CollapsibleView::new()
                    .open(advanced_open)
                    .motion_id(SharedString::from(format!("adv-{}", name(side))))
                    .content(
                        v_flex()
                            .child(subheading("Values the official app does not show", cx))
                            .child(slider_row_sub(
                                "Curve end",
                                Some("Stored with the curve; the official app never changes it"),
                                &end,
                                c.end.to_string(),
                                false,
                                cx,
                            ))
                            .child(subheading("Curve table: output + 50, recalculated whenever the curve changes", cx))
                            .children(table.iter().enumerate().map(|(i, st)| {
                                let v = s.curve_points.get(i).copied().unwrap_or(0);
                                slider_row_sub(table_label(i), None, st, v.to_string(), false, cx)
                            })),
                    ),
            )
            .when(copied_here, |b| b.child(div().absolute().top_0().left_0().size_full().occlude()));
        card(cx).child(header).child(body).into_any_element()
    }

    pub fn render_sticks(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let left = self.stick_card(&d, Side::Left, window, cx);
        let right = self.stick_card(&d, Side::Right, window, cx);
        columns(left, right, self.narrow).into_any_element()
    }
}
