use crate::prelude::*;
use flugdiggi_core::config::{Macro, Step};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable, h_flex, v_flex};
use gpui_kit::*;

use crate::app::FlugApp;
use crate::model::{
    self, BUTTONS, EVENT_LEFT_STICK, EVENT_PRESS, EVENT_RELEASE, EVENT_RIGHT_STICK, MAX_MACROS, STICK_DIRECTIONS,
    TARGETS, WAIT_SNAPS, key_label, macro_button, macro_steps_free,
};
use crate::theme::Colors;
use crate::widgets::{byte, card, heading, note, row, segmented, slider_row};

const MODES: [&str; 3] = ["Once", "Repeat while held", "Toggle repeat"];
const ACTIONS: [&str; 4] = ["Press", "Release", "Left stick", "Right stick"];

/// A step being dragged by its handle.
#[derive(Clone)]
struct DraggedStep {
    index: usize,
    label: SharedString,
}

impl Render for DraggedStep {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1p5()
            .rounded(cx.theme().radius)
            .bg(cx.theme().secondary)
            .text_sm()
            .shadow_md()
            .child(self.label.clone())
    }
}

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
        self.edit(cx, |d| model::edit_macro(&mut d.config, index, f));
    }

    pub fn render_macros(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let sel = self.macro_sel.min(MAX_MACROS.saturating_sub(1));
        let muted = cx.theme().muted_foreground;
        let this = cx.entity().downgrade();
        let empty = |i: usize| Macro {
            button: flugdiggi_core::config::parked_macro_button(i).unwrap_or_default(),
            mode: 1,
            steps: Vec::new(),
        };
        let macro_of = |i: usize| d.config.macros.list.get(i).cloned().unwrap_or_else(|| empty(i));
        let steps_text = |m: &Macro| match m.steps.len() {
            0 => "empty".to_owned(),
            1 => "1 step".to_owned(),
            n => format!("{n} steps"),
        };

        let items = (0..MAX_MACROS).map(|i| {
            let m = macro_of(i);
            let applied =
                macro_button(&d.config, i).map_or_else(|| "Not applied".to_owned(), |b| format!("on {}", key_label(b)));
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
                .child(
                    v_flex()
                        .child(div().text_sm().font_semibold().child(format!("Macro {}", i.saturating_add(1))))
                        .child(div().text_xs().text_color(muted).child(format!("{applied} · {}", steps_text(&m)))),
                )
        });
        let left = v_flex()
            .w(px(260.))
            .flex_shrink_0()
            .gap_2()
            .child(heading("Macros"))
            .child(card(cx).p_1().children(items))
            .child(note(
                "A macro runs when the button it is applied to is pressed. Apply it here or in the Buttons page's mapping menu.",
                cx,
            ));

        let m = macro_of(sel);
        let applied = macro_button(&d.config, sel);
        let free = macro_steps_free(&d.config, sel);
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

        let apply_menu = {
            let this = this.clone();
            let config = d.config.clone();
            Button::new("macro-button")
                .small()
                .w(px(170.))
                .label(applied.map_or_else(|| "Not applied".to_owned(), key_label))
                .dropdown_caret(true)
                .dropdown_menu(move |menu, _, _| {
                    let none = {
                        let this = this.clone();
                        PopupMenuItem::new("Not applied").checked(macro_button(&config, sel).is_none()).on_click(
                            move |_, _, cx| {
                                this.update(cx, |a, cx| a.edit(cx, |d| model::unapply_macro(&mut d.config, sel))).ok();
                            },
                        )
                    };
                    BUTTONS.iter().fold(menu.max_h(px(400.)).scrollable(true).item(none), |menu, &b| {
                        let this = this.clone();
                        let owner = model::macro_on_button(&config, b);
                        let label = match owner {
                            Some(n) if n != sel => format!("{} (Macro {})", key_label(b), n.saturating_add(1)),
                            _ => key_label(b),
                        };
                        menu.item(PopupMenuItem::new(label).checked(owner == Some(sel)).on_click(move |_, _, cx| {
                            this.update(cx, |a, cx| a.edit(cx, |d| model::apply_macro(&mut d.config, sel, b))).ok();
                        }))
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

        let waits = model::step_waits(&m);
        let mut step_rows = Vec::new();
        for (i, s) in m.steps.iter().enumerate() {
            let wait = waits.get(i).copied().unwrap_or(0);
            let wait_input = self.input(
                format!("step-wait-{sel}-{i}"),
                &wait.to_string(),
                move |a, text| {
                    if let Ok(ms) = text.trim().parse::<u32>() {
                        a.edit_silent(|d| {
                            model::edit_macro(&mut d.config, sel, |m| model::set_step_wait(m, i, ms));
                        });
                    }
                },
                window,
                cx,
            );
            let snaps = WAIT_SNAPS.len().saturating_sub(1);
            let wait_slider = self.slider(
                format!("step-wait-slider-{sel}-{i}"),
                (0.0, f32::from(u8::try_from(snaps).unwrap_or(0))),
                f32::from(u8::try_from(model::nearest_snap(wait)).unwrap_or(0)),
                move |a, v| {
                    let ms = WAIT_SNAPS.get(usize::from(byte(v))).copied().unwrap_or(0);
                    a.edit_silent(|d| {
                        model::edit_macro(&mut d.config, sel, |m| model::set_step_wait(m, i, ms));
                    });
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
            let label =
                format!("{} {}", ACTIONS.get(action_index(s.event)).copied().unwrap_or_default(), key_label(&s.key));
            step_rows.push(
                h_flex()
                    .id(("step", i))
                    .px_2()
                    .py_2()
                    .gap_2()
                    .when(i > 0, |r| r.border_t_1().border_color(Colors::line()))
                    .drag_over::<DraggedStep>(|style, _, _, cx| style.bg(cx.theme().secondary))
                    .on_drop(cx.listener(move |a, dragged: &DraggedStep, _, cx| {
                        let from = dragged.index;
                        a.edit_macro(cx, sel, |m| model::move_step(m, from, i));
                    }))
                    .child(
                        div()
                            .id(("grip", i))
                            .p_1()
                            .cursor_grab()
                            .text_color(muted)
                            .child(Icon::new(gpui_kit::assets::IconName::GripVertical).small())
                            .on_drag(DraggedStep { index: i, label: label.into() }, |d, _, _, cx| {
                                cx.new(|_| d.clone())
                            }),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1p5()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(div().w(px(36.)).text_xs().text_color(muted).child("Wait"))
                                    .child(div().w(px(72.)).child(Input::new(&wait_input).small()))
                                    .child(div().text_xs().text_color(muted).child("ms"))
                                    .child(div().flex_1().min_w(px(80.)).px_2().child(Slider::new(&wait_slider))),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(div().w(px(36.)).text_xs().text_color(muted).child("Then"))
                                    .child(action)
                                    .child(key_menu),
                            ),
                    )
                    .child(
                        Button::new(("rm-step", i))
                            .ghost()
                            .small()
                            .icon(IconName::Close)
                            .tooltip("Remove step")
                            .on_click(cx.listener(move |a, _, _, cx| {
                                a.edit_macro(cx, sel, |m| model::remove_step(m, i));
                            })),
                    ),
            );
        }
        let clearable = !m.steps.is_empty() || applied.is_some();

        let right = v_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(
                h_flex().justify_between().child(heading(format!("Macro {}", sel.saturating_add(1)))).child(
                    Button::new("clear-macro").small().label("Clear macro").disabled(!clearable).on_click(cx.listener(
                        move |a, _, _, cx| {
                            a.edit(cx, |d| model::clear_macro(&mut d.config, sel));
                        },
                    )),
                ),
            )
            .child(
                card(cx)
                    .child(
                        row("Applied to", Some("Pressing this button plays the macro instead"), apply_menu, cx)
                            .border_t_0(),
                    )
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
                    .when(m.mode >= 2, |c| {
                        c.child(slider_row("Pause between repeats", &pause, format!("{interval} ms"), false, cx))
                    }),
            )
            .child(heading("Timeline").mt_2())
            .child(card(cx).child(if labels.is_empty() {
                div().p_3().text_sm().text_color(muted).child("Add steps to see them here.").into_any_element()
            } else {
                timeline.into_any_element()
            }))
            .child(
                h_flex().mt_2().justify_between().child(heading("Steps")).child(
                    Button::new("add-step")
                        .small()
                        .icon(IconName::Plus)
                        .label("Add step")
                        .disabled(free == 0)
                        .on_click(cx.listener(move |a, _, _, cx| a.edit_macro(cx, sel, model::add_step))),
                ),
            )
            .when(!step_rows.is_empty(), |r| r.child(card(cx).children(step_rows)))
            .child(note(
                format!(
                    "Each step waits the given time after the previous one (the first after the button press), in steps of 10 ms. Drag a step by its handle to move it. Stick steps hold the stick until the next step for that stick. All macros share space for {free} more steps."
                ),
                cx,
            ));

        h_flex().items_start().gap_4().child(left).child(right).into_any_element()
    }
}
