use crate::prelude::*;
use flugdiggi_core::config::Turbo;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::*;

use crate::app::FlugApp;
use crate::model::{
    self, BUTTONS, MAX_MACROS, Page, TARGETS, is_extra, key_label, macro_button, macro_on_button, turbo_of,
};
use crate::theme::Colors;
use crate::widgets::{byte, card, columns, edited_dot, segmented};

const TURBO_MODES: [&str; 3] = ["off", "hold", "toggle"];

fn turbo_label(t: Option<&Turbo>) -> String {
    match t {
        Some(t) if t.mode == "hold" => format!("Hold · {}/s", t.rate),
        Some(t) if t.mode == "toggle" => format!("Toggle · {}/s", t.rate),
        _ => "Off".to_owned(),
    }
}

impl FlugApp {
    pub fn render_buttons(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let this = cx.entity().downgrade();
        let muted = cx.theme().muted_foreground;
        let mut rows = Vec::new();
        for (i, &button) in BUTTONS.iter().enumerate() {
            let mapping = d.config.buttons.get(button).cloned();
            let saved_mapping = d.saved_config.buttons.get(button).cloned();
            let turbo = turbo_of(&d.config, button).cloned();
            let macro_slot = macro_on_button(&d.config, button);
            let has_macro = macro_slot.is_some();
            let macro_entries: Vec<(String, bool)> = (0..MAX_MACROS)
                .map(|n| {
                    let here = macro_slot == Some(n);
                    let state = match (macro_button(&d.config, n), d.config.macros.list.get(n)) {
                        (Some(b), _) if !here => format!(" (on {})", key_label(b)),
                        (None, m) if m.is_none_or(|m| m.steps.is_empty()) => " (empty)".to_owned(),
                        _ => String::new(),
                    };
                    (format!("Macro {}{state}", n.saturating_add(1)), here)
                })
                .collect();
            let edited = mapping != saved_mapping || turbo.as_ref() != turbo_of(&d.saved_config, button);
            let changed = mapping.is_some() || turbo.is_some();
            let rate_value = f32::from(turbo.as_ref().map_or(15, |t| t.rate));
            let rate = self.slider(
                format!("turbo-rate-{button}"),
                (1.0, 30.0),
                rate_value,
                move |app, v| {
                    app.edit_silent(|d| {
                        let mode = turbo_of(&d.config, button).map_or_else(|| "hold".to_owned(), |t| t.mode.clone());
                        model::set_turbo(&mut d.config, button, Some(Turbo { rate: byte(v), mode }));
                    });
                },
                window,
                cx,
            );

            let map_label = match mapping.as_deref() {
                _ if has_macro => format!("Macro {}", macro_slot.map_or(0, |n| n.saturating_add(1))),
                None if is_extra(button) => "Nothing".to_owned(),
                None => "Default".to_owned(),
                Some(t) => key_label(t),
            };
            let map_menu = {
                let this = this.clone();
                let mapping = mapping.clone();
                move |menu: gpui_kit::component::menu::PopupMenu,
                      _: &mut Window,
                      _: &mut Context<gpui_kit::component::menu::PopupMenu>| {
                    let set = |target: Option<&'static str>| {
                        let this = this.clone();
                        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                            this.update(cx, |app, cx| {
                                app.edit(cx, |d| {
                                    if let Some(m) = macro_on_button(&d.config, button) {
                                        model::unapply_macro(&mut d.config, m);
                                    }
                                    model::set_mapping(&mut d.config, button, target);
                                });
                            })
                            .ok();
                        }
                    };
                    let default_label = if is_extra(button) { "Nothing (default)" } else { "Default" };
                    let mut menu = menu
                        .max_h(px(420.))
                        .scrollable(true)
                        .item(
                            PopupMenuItem::new(default_label)
                                .checked(mapping.is_none() && !has_macro)
                                .on_click(set(None)),
                        )
                        .separator();
                    for t in TARGETS.iter().filter(|t| **t != button) {
                        menu = menu.item(
                            PopupMenuItem::new(key_label(t))
                                .checked(mapping.as_deref() == Some(*t))
                                .on_click(set(Some(*t))),
                        );
                    }
                    if !is_extra(button) {
                        menu = menu.item(
                            PopupMenuItem::new("Nothing")
                                .checked(mapping.as_deref() == Some("none"))
                                .on_click(set(Some("none"))),
                        );
                    }
                    macro_entries.iter().enumerate().fold(menu.separator(), |menu, (n, (label, here))| {
                        let this = this.clone();
                        menu.item(PopupMenuItem::new(label.clone()).checked(*here).on_click(move |_, _, cx| {
                            this.update(cx, |app, cx| {
                                app.edit(cx, |d| model::apply_macro(&mut d.config, n, button));
                            })
                            .ok();
                        }))
                    })
                }
            };
            let mapped = h_flex()
                .w(px(170.))
                .gap_1()
                .child(
                    Button::new(("map", i))
                        .w(px(if has_macro { 136. } else { 170. }))
                        .small()
                        .label(map_label)
                        .dropdown_caret(true)
                        .when(mapping.is_none() && !has_macro, |b| b.text_color(muted))
                        .dropdown_menu(map_menu),
                )
                .when(has_macro, |row| {
                    row.child(
                        Button::new(("edit-macro", i))
                            .small()
                            .icon(Icon::new(gpui_kit::assets::IconName::Pencil))
                            .tooltip("Edit macro")
                            .on_click(cx.listener(move |app, _, _, cx| {
                                if let Some(m) = app.current().and_then(|d| macro_on_button(&d.config, button)) {
                                    app.macro_sel = m;
                                }
                                app.page = Page::Macros;
                                cx.notify();
                            })),
                    )
                });

            let turbo_cell = if has_macro {
                div().w(px(140.)).into_any_element()
            } else {
                let this = this.clone();
                let rate = rate.clone();
                Popover::new(("turbo", i))
                    .anchor(Anchor::TopLeft)
                    .trigger(
                        Button::new(("turbo-btn", i))
                            .w(px(140.))
                            .small()
                            .label(turbo_label(turbo.as_ref()))
                            .dropdown_caret(true)
                            .when(turbo.is_none(), |b| b.text_color(muted)),
                    )
                    .content(move |_, _, cx| {
                        let current = this
                            .upgrade()
                            .and_then(|a| a.read(cx).current().and_then(|d| turbo_of(&d.config, button).cloned()));
                        let mode =
                            current.as_ref().and_then(|t| TURBO_MODES.iter().position(|m| *m == t.mode)).unwrap_or(0);
                        let rate_text = format!("{} presses/s", current.as_ref().map_or(15, |t| t.rate));
                        let set_mode = {
                            let this = this.clone();
                            move |m: usize, _: &mut Window, cx: &mut App| {
                                this.update(cx, |app, cx| {
                                    app.edit(cx, |d| {
                                        let rate = turbo_of(&d.config, button).map_or(15, |t| t.rate);
                                        let turbo = TURBO_MODES
                                            .get(m)
                                            .filter(|m| **m != "off")
                                            .map(|m| Turbo { rate, mode: (*m).to_owned() });
                                        model::set_turbo(&mut d.config, button, turbo);
                                    });
                                })
                                .ok();
                            }
                        };
                        v_flex()
                            .w(px(240.))
                            .gap_3()
                            .text_sm()
                            .child(
                                segmented(("turbo-mode", i), &["Off", "Hold", "Toggle"], Some(mode), set_mode).w_full(),
                            )
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(h_flex().justify_between().child("Rate").child(
                                        div().text_xs().text_color(cx.theme().muted_foreground).child(rate_text),
                                    ))
                                    .child(Slider::new(&rate).disabled(mode == 0)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Hold: repeats while held. Toggle: press once to start, again to stop."),
                            )
                    })
                    .into_any_element()
            };

            rows.push(
                h_flex()
                    .min_h(px(38.))
                    .px_2p5()
                    .gap_2p5()
                    .when(i % 11 != 0, |r| r.border_t_1().border_color(Colors::line()))
                    .child(
                        div()
                            .w(px(48.))
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().secondary)
                            .text_xs()
                            .font_semibold()
                            .child(key_label(button)),
                    )
                    .child(mapped)
                    .child(turbo_cell)
                    .child(h_flex().flex_1().justify_end().gap_1p5().when(edited, |r| r.child(edited_dot())).when(
                        changed || has_macro,
                        |r| {
                            r.child(
                                Button::new(("reset", i))
                                    .ghost()
                                    .small()
                                    .icon(Icon::new(gpui_kit::assets::IconName::RotateCcw))
                                    .tooltip("Reset to default")
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        app.edit(cx, |d| {
                                            if let Some(m) = macro_on_button(&d.config, button) {
                                                model::unapply_macro(&mut d.config, m);
                                            }
                                            model::set_mapping(&mut d.config, button, None);
                                            model::set_turbo(&mut d.config, button, None);
                                        });
                                    })),
                            )
                        },
                    ))
                    .into_any_element(),
            );
        }
        let mut second = rows.split_off(11);
        let column = |rows: Vec<AnyElement>, cx: &App| {
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1p5()
                .child(
                    h_flex()
                        .px_2p5()
                        .gap_2p5()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child(div().w(px(48.)).child("Button"))
                        .child(div().w(px(170.)).child("Mapped to"))
                        .child(div().child("Turbo")),
                )
                .child(card(cx).children(rows))
        };
        second.truncate(11);
        columns(column(rows, cx), column(second, cx), self.narrow).into_any_element()
    }
}
