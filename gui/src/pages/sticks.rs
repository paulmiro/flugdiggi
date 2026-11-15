use crate::prelude::*;
use flugdiggi_core::config::{Curve, Stick};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::collapsible::Collapsible as CollapsibleView;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::*;

use super::curve::{CURVE_KINDS, chart};
use crate::app::FlugApp;
use crate::model::SlotData;
use crate::widgets::{byte, card, columns, edited_dot, percent, row, segmented, slider_row, subheading};

/// Raw maximum of stick curve values.
const STICK_MAX: u8 = 127;

#[derive(Clone, Copy)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    fn index(self) -> usize {
        match self {
            Side::Left => 0,
            Side::Right => 1,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
}

fn stick(d: &mut SlotData, side: Side) -> &mut Stick {
    match side {
        Side::Left => &mut d.config.left_stick,
        Side::Right => &mut d.config.right_stick,
    }
}

/// Applies `f` to the stick, and to the other one too while the sticks are linked.
fn edit_stick(app: &mut FlugApp, side: Side, f: impl Fn(&mut Stick)) {
    let linked = app.link_sticks;
    app.edit_silent(|d| {
        f(stick(d, side));
        if linked {
            f(stick(
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
    fn stick_card(&mut self, d: &SlotData, side: Side, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (s, saved) = match side {
            Side::Left => (&d.config.left_stick, &d.saved_config.left_stick),
            Side::Right => (&d.config.right_stick, &d.saved_config.right_stick),
        };
        let n = side.name();
        let c = s.curve.clone();
        let deadzone = self.slider(
            format!("{n}-dz"),
            (0.0, 127.0),
            f32::from(c.start),
            move |a, v| {
                edit_stick(a, side, |s| s.curve.start = byte(v));
            },
            window,
            cx,
        );
        let outer = self.slider(
            format!("{n}-end"),
            (0.0, 127.0),
            f32::from(c.end),
            move |a, v| {
                edit_stick(a, side, |s| s.curve.end = byte(v));
            },
            window,
            cx,
        );
        let point = |app: &mut FlugApp,
                     key: &str,
                     value: u8,
                     f: fn(&mut Curve, u8),
                     window: &mut Window,
                     cx: &mut Context<Self>| {
            app.slider(
                format!("{n}-{key}"),
                (0.0, 127.0),
                f32::from(value),
                move |a, v| {
                    edit_stick(a, side, |s| f(&mut s.curve, byte(v)));
                },
                window,
                cx,
            )
        };
        let p1x = point(self, "p1x", c.point1[0], |c, v| c.point1[0] = v, window, cx);
        let p1y = point(self, "p1y", c.point1[1], |c, v| c.point1[1] = v, window, cx);
        let p2x = point(self, "p2x", c.point2[0], |c, v| c.point2[0] = v, window, cx);
        let p2y = point(self, "p2y", c.point2[1], |c, v| c.point2[1] = v, window, cx);
        let edge = self.slider(
            format!("{n}-edge"),
            (0.0, 127.0),
            f32::from(s.edge),
            move |a, v| {
                edit_stick(a, side, |s| s.edge = byte(v));
            },
            window,
            cx,
        );
        let mut ext = Vec::new();
        for (i, v) in s.curve_points.iter().enumerate() {
            ext.push(self.slider(
                format!("{n}-ext{i}"),
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
        let advanced_open = self.advanced_open(&format!("stick-{n}"));
        let edited = s != saved;
        let title = match side {
            Side::Left => "Left stick",
            Side::Right => "Right stick",
        };
        let kind = usize::from(c.kind).min(3);
        let saved_stick = saved.clone();
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
                        Button::new(("reset-stick", side.index()))
                            .ghost()
                            .small()
                            .icon(Icon::new(gpui_kit::assets::IconName::RotateCcw))
                            .tooltip("Undo changes to this stick")
                            .disabled(!edited)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                let saved = saved_stick.clone();
                                app.edit(cx, |d| *stick(d, side) = saved);
                            })),
                    ),
            )
            .child(div().p_3().child(chart(&c, STICK_MAX, c != saved.curve)))
            .child(
                div().px_3().pb_3().child(
                    segmented(("stick-kind", side.index()), &CURVE_KINDS, Some(kind), {
                        let this = cx.entity().downgrade();
                        move |k, _, cx| {
                            this.update(cx, |a, cx| {
                                edit_stick(a, side, |s| s.curve.kind = u8::try_from(k).unwrap_or(0));
                                cx.notify();
                            })
                            .ok();
                        }
                    })
                    .w_full(),
                ),
            )
            .child(slider_row("Deadzone", &deadzone, percent(c.start, STICK_MAX), false, cx))
            .child(slider_row("Outer limit", &outer, percent(c.end, STICK_MAX), false, cx))
            .when(kind == 3, |card| {
                card.child(slider_row("Point 1 input", &p1x, percent(c.point1[0], STICK_MAX), false, cx))
                    .child(slider_row("Point 1 output", &p1y, percent(c.point1[1], STICK_MAX), false, cx))
                    .child(slider_row("Point 2 input", &p2x, percent(c.point2[0], STICK_MAX), false, cx))
                    .child(slider_row("Point 2 output", &p2y, percent(c.point2[1], STICK_MAX), false, cx))
            })
            .child(row(
                "Range shape",
                Some("How far the stick reaches diagonally"),
                segmented(("circ", side.index()), &["Square", "Round"], Some(usize::from(s.circularity.min(1))), {
                    let this = cx.entity().downgrade();
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
            .child(self.advanced_toggle(format!("stick-{n}"), cx))
            .child(
                CollapsibleView::new().open(advanced_open).motion_id(SharedString::from(format!("adv-{n}"))).content(
                    v_flex()
                        .child(subheading("Values the official app does not show", cx))
                        .child(slider_row("Edge", &edge, s.edge.to_string(), false, cx))
                        .children(ext.iter().enumerate().map(|(i, st)| {
                            let v = s.curve_points.get(i).copied().unwrap_or(0);
                            slider_row(
                                format!("Extended curve point {}", i.saturating_add(1)),
                                st,
                                v.to_string(),
                                false,
                                cx,
                            )
                        })),
                ),
            )
            .into_any_element()
    }

    pub fn render_sticks(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let left = self.stick_card(&d, Side::Left, window, cx);
        let right = self.stick_card(&d, Side::Right, window, cx);
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
                            .child("While linked, changes to one stick apply to both."),
                    )
                    .child(Switch::new("link-sticks").label("Link sticks").checked(self.link_sticks).on_click(
                        cx.listener(|a, v: &bool, _, cx| {
                            a.link_sticks = *v;
                            cx.notify();
                        }),
                    )),
            )
            .child(columns(left, right))
            .into_any_element()
    }
}
