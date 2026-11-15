use crate::prelude::*;
use flugdiggi_core::config::{Macro, Step};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, IconName, Sizable, h_flex, v_flex};
use gpui_kit::*;

use crate::app::FlugApp;
use crate::model::{
    self, BUTTONS, EVENT_LEFT_STICK, EVENT_PRESS, EVENT_RELEASE, EVENT_RIGHT_STICK, MAX_MACROS, STICK_DIRECTIONS,
    TARGETS, key_label, macro_index,
};
use crate::theme::Colors;
use crate::widgets::{byte, card, heading, note, row, segmented, slider_row};

const MODES: [&str; 3] = ["Once", "Repeat while held", "Toggle repeat"];
const ACTIONS: [&str; 4] = ["Press", "Release", "Left stick", "Right stick"];

fn action_index(event: u8) -> usize {
    match event {
        EVENT_RELEASE => 1,
        EVENT_LEFT_STICK => 2,
        EVENT_RIGHT_STICK => 3,
        _ => 0,
    }
}

fn action_event(index: usize) -> u8 {
    match index {
        1 => EVENT_RELEASE,
        2 => EVENT_LEFT_STICK,
        3 => EVENT_RIGHT_STICK,
        _ => EVENT_PRESS,
    }
}

fn is_stick(event: u8) -> bool {
    matches!(event, EVENT_LEFT_STICK | EVENT_RIGHT_STICK)
}

fn mode_text(m: &Macro) -> &'static str {
    match m.mode {
        2 => "repeat while held",
        3 => "toggle repeat",
        _ => "once",
    }
}

/// Bars of the timeline: (row label, start ms, end ms).
fn timeline_bars(m: &Macro) -> (Vec<String>, Vec<(usize, u32, u32)>, u32) {
    let end = m.steps.iter().map(|s| s.at_ms).max().unwrap_or(0).max(10);
    let mut labels: Vec<String> = Vec::new();
    let mut bars = Vec::new();
    let mut open: Vec<(String, u32)> = Vec::new();
    let row_of = |label: String, labels: &mut Vec<String>| {
        labels.iter().position(|l| *l == label).unwrap_or_else(|| {
            labels.push(label);
            labels.len().saturating_sub(1)
        })
    };
    let mut steps: Vec<&Step> = m.steps.iter().collect();
    steps.sort_by_key(|s| s.at_ms);
    for s in steps {
        let label = match s.event {
            EVENT_LEFT_STICK => "Left stick".to_owned(),
            EVENT_RIGHT_STICK => "Right stick".to_owned(),
            _ => key_label(&s.key),
        };
        if let Some(i) = open.iter().position(|(l, _)| *l == label) {
            let (l, start) = open.remove(i);
            let r = row_of(l, &mut labels);
            bars.push((r, start, s.at_ms));
        }
        let starts = s.event == EVENT_PRESS || (is_stick(s.event) && s.key != "stick_center");
        if starts {
            row_of(label.clone(), &mut labels);
            open.push((label, s.at_ms));
        }
    }
    for (l, start) in open {
        let r = row_of(l, &mut labels);
        bars.push((r, start, end));
    }
    (labels, bars, end)
}

impl FlugApp {
    fn edit_macro(&mut self, cx: &mut Context<Self>, index: usize, f: impl FnOnce(&mut Macro)) {
        self.edit(cx, |d| {
            if let Some(m) = d.config.macros.list.get_mut(index) {
                f(m);
            }
        });
    }

    pub fn render_macros(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let list = &d.config.macros.list;
        let sel = self.macro_sel.min(list.len().saturating_sub(1));
        let muted = cx.theme().muted_foreground;
        let this = cx.entity().downgrade();

        let items = list.iter().enumerate().map(|(i, m)| {
            div()
                .id(("macro", i))
                .px_3()
                .py_2()
                .rounded(cx.theme().radius)
                .cursor_pointer()
                .when(i == sel, |d| d.bg(cx.theme().secondary))
                .on_click(cx.listener(move |a, _, _, cx| {
                    a.macro_sel = i;
                    cx.notify();
                }))
                .child(h_flex().gap_2().text_sm().child(div().font_semibold().child(key_label(&m.button))).child(
                    div().text_xs().text_color(muted).child(format!(
                        "{} · {} ms · {}",
                        if m.steps.len() == 1 { "1 step".to_owned() } else { format!("{} steps", m.steps.len()) },
                        m.steps.iter().map(|s| s.at_ms).max().unwrap_or(0),
                        mode_text(m)
                    )),
                ))
        });
        let left = v_flex()
            .w(px(260.))
            .flex_shrink_0()
            .gap_2()
            .child(heading("Macros"))
            .when(!list.is_empty(), |l| l.child(card(cx).p_1().children(items)))
            .child(
                Button::new("new-macro")
                    .small()
                    .icon(IconName::Plus)
                    .label("New macro")
                    .disabled(list.len() >= MAX_MACROS)
                    .on_click(cx.listener(|a, _, _, cx| {
                        let mut index = None;
                        a.edit(cx, |d| index = model::add_macro(&mut d.config));
                        if let Some(i) = index {
                            a.macro_sel = i;
                        }
                    })),
            )
            .child(note(
                format!("Up to {MAX_MACROS} macros. A button with a macro shows \"Macro\" on the Buttons page."),
                cx,
            ));

        let Some(m) = list.get(sel).cloned() else {
            return h_flex()
                .items_start()
                .gap_4()
                .child(left)
                .child(
                    v_flex()
                        .flex_1()
                        .pt_8()
                        .items_center()
                        .text_sm()
                        .text_color(muted)
                        .child("No macros in this slot."),
                )
                .into_any_element();
        };

        let interval = d.config.macros.intervals_ms.get(sel).copied().unwrap_or(0);
        let pause = self.slider(
            format!("macro-pause-{sel}"),
            (0.0, 255.0),
            f32::from(interval / 10),
            move |a, v| {
                a.edit_silent(|d| {
                    if let Some(i) = d.config.macros.intervals_ms.get_mut(sel) {
                        *i = u16::from(byte(v)).saturating_mul(10);
                    }
                });
            },
            window,
            cx,
        );

        let button_menu = {
            let this = this.clone();
            let current = m.button.clone();
            let config = d.config.clone();
            Button::new("macro-button")
                .small()
                .w(px(130.))
                .label(key_label(&m.button))
                .dropdown_caret(true)
                .dropdown_menu(move |menu, _, _| {
                    BUTTONS.iter().fold(menu.max_h(px(400.)).scrollable(true), |menu, &b| {
                        let this = this.clone();
                        let taken = b != current && macro_index(&config, b).is_some();
                        menu.item(PopupMenuItem::new(key_label(b)).checked(b == current).disabled(taken).on_click(
                            move |_, _, cx| {
                                this.update(cx, |a, cx| a.edit(cx, |d| model::set_macro_button(&mut d.config, sel, b)))
                                    .ok();
                            },
                        ))
                    })
                })
        };

        let (labels, bars, end) = timeline_bars(&m);
        let end_f = f64::from(end).max(1.0);
        let frac = |ms: u32| -> f32 {
            let f = f64::from(ms) / end_f;
            // Fractions only, so the narrowing is exact enough for layout.
            #[allow(clippy::as_conversions, clippy::cast_possible_truncation)]
            let f = f as f32;
            f.clamp(0.0, 1.0)
        };
        let timeline = v_flex()
            .p_3()
            .gap_1()
            .children(labels.iter().enumerate().map(|(r, label)| {
                h_flex()
                    .gap_2()
                    .h(px(20.))
                    .child(div().w(px(80.)).text_xs().text_color(muted).child(label.clone()))
                    .child(div().flex_1().h_full().relative().bg(cx.theme().background).rounded(px(4.)).children(
                        bars.iter().filter(|(row, _, _)| *row == r).map(|&(_, s, e)| {
                            let (s, e) = (frac(s), frac(e));
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(relative(s))
                                .w(relative((e - s).max(0.01)))
                                .rounded(px(4.))
                                .bg(Colors::edited().opacity(0.7))
                        }),
                    ))
            }))
            .child(
                h_flex()
                    .pl(px(88.))
                    .justify_between()
                    .text_xs()
                    .text_color(muted)
                    .child("0")
                    .child(format!("{end} ms")),
            );

        let mut step_rows = Vec::new();
        for (i, s) in m.steps.iter().enumerate() {
            let at = self.input(
                format!("step-at-{sel}-{i}"),
                &s.at_ms.to_string(),
                move |a, text| {
                    if let Ok(ms) = text.trim().parse::<u32>() {
                        let ms = ms.min(655_350).saturating_div(10).saturating_mul(10);
                        a.edit_silent(|d| {
                            if let Some(step) = d.config.macros.list.get_mut(sel).and_then(|m| m.steps.get_mut(i)) {
                                step.at_ms = ms;
                            }
                        });
                    }
                },
                window,
                cx,
            );
            let stick = is_stick(s.event);
            let keys: Vec<&'static str> = if stick { STICK_DIRECTIONS.to_vec() } else { TARGETS.to_vec() };
            let key_menu = {
                let this = this.clone();
                let current = s.key.clone();
                Button::new(("step-key", i))
                    .small()
                    .w(px(110.))
                    .label(key_label(&s.key))
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        keys.iter().fold(menu.max_h(px(400.)).scrollable(true), |menu, &k| {
                            let this = this.clone();
                            menu.item(PopupMenuItem::new(key_label(k)).checked(current == k).on_click(
                                move |_, _, cx| {
                                    this.update(cx, |a, cx| {
                                        a.edit_macro(cx, sel, |m| {
                                            if let Some(step) = m.steps.get_mut(i) {
                                                k.clone_into(&mut step.key);
                                            }
                                        });
                                    })
                                    .ok();
                                },
                            ))
                        })
                    })
            };
            let action = segmented(("step-action", i), &ACTIONS, Some(action_index(s.event)), {
                let this = this.clone();
                move |k, _, cx| {
                    this.update(cx, |a, cx| {
                        a.edit_macro(cx, sel, |m| {
                            if let Some(step) = m.steps.get_mut(i) {
                                let was_stick = is_stick(step.event);
                                step.event = action_event(k);
                                if is_stick(step.event) != was_stick {
                                    let key = if is_stick(step.event) { "stick_center" } else { "a" };
                                    key.clone_into(&mut step.key);
                                }
                            }
                        });
                    })
                    .ok();
                }
            });
            step_rows.push(
                h_flex()
                    .min_h(px(40.))
                    .px_3()
                    .gap_2()
                    .when(i > 0, |r| r.border_t_1().border_color(Colors::line()))
                    .child(
                        h_flex()
                            .w(px(110.))
                            .gap_1()
                            .child(Input::new(&at).small())
                            .child(div().text_xs().text_color(muted).child("ms")),
                    )
                    .child(key_menu)
                    .child(div().flex_1().child(action))
                    .child(
                        Button::new(("rm-step", i))
                            .ghost()
                            .small()
                            .icon(IconName::Close)
                            .tooltip("Remove step")
                            .on_click(cx.listener(move |a, _, _, cx| {
                                a.edit_macro(cx, sel, |m| {
                                    if i < m.steps.len() {
                                        m.steps.remove(i);
                                    }
                                });
                            })),
                    ),
            );
        }
        let last_at = m.steps.iter().map(|s| s.at_ms).max();

        let right = v_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(
                h_flex()
                    .justify_between()
                    .child(heading(format!("Macro on {}", key_label(&m.button))))
                    .child(Button::new("delete-macro").small().label("Delete macro").on_click(cx.listener(move |a, _, _, cx| {
                        a.edit(cx, |d| model::remove_macro(&mut d.config, sel));
                        a.macro_sel = sel.saturating_sub(1);
                    }))),
            )
            .child(
                card(cx)
                    .child(row("Button", Some("Pressing it plays the macro instead"), button_menu, cx).border_t_0())
                    .child(row(
                        "Plays",
                        None,
                        segmented("macro-mode", &MODES, Some(usize::from(m.mode.clamp(1, 3).saturating_sub(1))), {
                            let this = this.clone();
                            move |k, _, cx| {
                                let mode = u8::try_from(k.saturating_add(1)).unwrap_or(1);
                                this.update(cx, |a, cx| a.edit_macro(cx, sel, |m| m.mode = mode)).ok();
                            }
                        }),
                        cx,
                    ))
                    .when(m.mode >= 2, |c| c.child(slider_row("Pause between repeats", &pause, format!("{interval} ms"), false, cx))),
            )
            .child(heading("Timeline").mt_2())
            .child(card(cx).child(if labels.is_empty() {
                div().p_3().text_sm().text_color(muted).child("Add steps to see them here.").into_any_element()
            } else {
                timeline.into_any_element()
            }))
            .child(
                h_flex().mt_2().justify_between().child(heading("Steps")).child(
                    Button::new("add-step").small().icon(IconName::Plus).label("Add step").on_click(cx.listener(move |a, _, _, cx| {
                        let at = last_at.map_or(0, |t| t.saturating_add(40));
                        a.edit_macro(cx, sel, |m| m.steps.push(Step { at_ms: at, key: "a".to_owned(), event: EVENT_PRESS }));
                    })),
                ),
            )
            .when(!step_rows.is_empty(), |r| r.child(card(cx).children(step_rows)))
            .child(note("Times count from the button press, in steps of 10 ms. Stick steps hold the stick until the next step for that stick.", cx));

        h_flex().items_start().gap_4().child(left).child(right).into_any_element()
    }
}
