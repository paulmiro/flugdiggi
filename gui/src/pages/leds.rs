use crate::prelude::*;
use gpui_kit::component::button::Button;
use gpui_kit::component::collapsible::Collapsible as CollapsibleView;
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{ActiveTheme, IconName, Sizable, h_flex, v_flex};
use gpui_kit::*;

use crate::app::FlugApp;
use crate::model::{
    self, FRAMES, LED_ORDER, LedPos, PRESETS, frame_count, hsv_to_rgb, led_color, parse_hex, preset_name, rgb_to_hsv,
    set_led_color,
};
use crate::widgets::{byte, card, heading, row, segmented, slider_row, subheading};

pub fn hex_hsla(s: &str) -> Hsla {
    let (r, g, b) = parse_hex(s).unwrap_or((0, 0, 0));
    rgb(u32::from_be_bytes([0, r, g, b])).into()
}

fn hsv_hex((h, s, v): (f32, f32, f32)) -> String {
    let (r, g, b) = hsv_to_rgb(h, s, v);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Inserts a copy of `frame` after it; the last stored frame drops off.
fn duplicate_frame(led: &mut flugdiggi_core::led::Led, frame: usize) {
    for pos in LED_ORDER {
        let steps = model::led_steps_mut(&mut led.leds, pos);
        if let Some(c) = steps.get(frame).cloned() {
            steps.insert(frame.saturating_add(1), c);
            steps.truncate(FRAMES);
        }
    }
}

fn remove_frame(led: &mut flugdiggi_core::led::Led, frame: usize) {
    for pos in LED_ORDER {
        let steps = model::led_steps_mut(&mut led.leds, pos);
        if frame < steps.len() {
            steps.remove(frame);
            steps.push("#000000".to_owned());
        }
    }
}

/// A colour cell of the frame grid; opens the colour picker.
#[derive(IntoElement)]
struct Cell {
    id: ElementId,
    color: Hsla,
    selected: bool,
}

impl gpui_kit::component::Selectable for Cell {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for Cell {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .w_full()
            .h(px(32.))
            .rounded(px(6.))
            .cursor_pointer()
            .bg(self.color)
            .when(self.selected, |d| d.border_2().border_color(rgb(0x00ec_ecec)))
    }
}

/// The LED strip as a V, each LED's colour blending into its neighbours.
fn preview(colors: [Hsla; 5]) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            let (x0, y0) = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
            let (w, h) = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
            let at = |t: f32| {
                // 0..0.5 down the left arm, 0.5..1 up the right arm.
                let x = x0 + w * (0.06 + 0.88 * t);
                let depth = 1.0 - (t - 0.5).abs() * 2.0;
                point(px(x), px(y0 + h * (0.12 + 0.76 * depth)))
            };
            let steps = 60u8;
            for i in 0..steps {
                let (t0, t1) = (f32::from(i) / f32::from(steps), f32::from(i.saturating_add(1)) / f32::from(steps));
                let tm = t0.midpoint(t1);
                // LED k sits at t = k / 4; colours hold near an LED and blend between them.
                let pos = tm * 4.0;
                let k = pos.floor().clamp(0.0, 3.0);
                let frac = pos - k;
                let blend = ((frac - 0.3) / 0.4).clamp(0.0, 1.0);
                let k = usize::from(model::unit_to_u8(k));
                let a = colors.get(k).copied().unwrap_or_default();
                let b = colors.get(k.saturating_add(1)).copied().unwrap_or(a);
                let c = Hsla::from(Rgba::from(a).blend(Rgba { a: blend, ..Rgba::from(b) }));
                let mut p = PathBuilder::stroke(px(12.));
                p.move_to(at(t0));
                p.line_to(at(t1));
                if let Ok(path) = p.build() {
                    window.paint_path(path, c);
                }
            }
        },
    )
    .w(px(360.))
    .h(px(120.))
}

impl FlugApp {
    pub fn render_leds(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let led = &d.led;
        let frames = frame_count(led);
        let selected = self.led_frame.min(frames.saturating_sub(1));

        let hsv = self.picker_hsv;
        let hue = self.slider("pick-h", (0.0, 359.0), hsv.0, |a, v| a.set_picker_hsv(|h| h.0 = v), window, cx);
        let sat = self.slider(
            "pick-s",
            (0.0, 100.0),
            hsv.1 * 100.0,
            |a, v| a.set_picker_hsv(|h| h.1 = v / 100.0),
            window,
            cx,
        );
        let val = self.slider(
            "pick-v",
            (0.0, 100.0),
            hsv.2 * 100.0,
            |a, v| a.set_picker_hsv(|h| h.2 = v / 100.0),
            window,
            cx,
        );
        let picked = self.picker.map_or_else(String::new, |(f, p)| led_color(led, f, p).to_owned());
        let hex = self.input(
            "pick-hex",
            &picked,
            |a, text| {
                if let Some((r, g, b)) = parse_hex(text.trim()) {
                    a.picker_hsv = rgb_to_hsv(r, g, b);
                    let color = format!("#{r:02x}{g:02x}{b:02x}");
                    a.set_picked_color(&color);
                }
            },
            window,
            cx,
        );
        let brightness = self.slider(
            "led-bright",
            (1.0, 100.0),
            f32::from(led.brightness),
            |a, v| {
                a.edit_silent(|d| d.led.brightness = byte(v));
            },
            window,
            cx,
        );
        let step = self.slider(
            "led-step",
            (1.0, 100.0),
            f32::from(led.step_time),
            |a, v| {
                a.edit_silent(|d| d.led.step_time = byte(v));
            },
            window,
            cx,
        );
        let loop_start = self.slider(
            "led-loop-start",
            (0.0, 9.0),
            f32::from(led.loop_start),
            |a, v| {
                a.edit_silent(|d| d.led.loop_start = byte(v));
            },
            window,
            cx,
        );
        let led_count = self.slider(
            "led-count",
            (0.0, 16.0),
            f32::from(led.led_count),
            |a, v| {
                a.edit_silent(|d| d.led.led_count = byte(v));
            },
            window,
            cx,
        );

        let mut palette: Vec<String> = Vec::new();
        for f in 0..frames {
            for p in LED_ORDER {
                let c = led_color(led, f, p).to_owned();
                if !palette.contains(&c) {
                    palette.push(c);
                }
            }
        }
        palette.truncate(10);

        let this = cx.entity().downgrade();
        let grid = v_flex().gap_1p5().children((0..frames).map(|f| {
            let number_color = if f == selected { cx.theme().foreground } else { cx.theme().muted_foreground };
            h_flex()
                .gap_1p5()
                .child(
                    div()
                        .id(("frame-n", f))
                        .w(px(32.))
                        .text_xs()
                        .font_semibold()
                        .cursor_pointer()
                        .text_color(number_color)
                        .child(f.saturating_add(1).to_string())
                        .on_click(cx.listener(move |a, _, _, cx| {
                            a.led_frame = f;
                            cx.notify();
                        })),
                )
                .children(LED_ORDER.iter().enumerate().map(|(i, &pos)| {
                    let color = led_color(led, f, pos).to_owned();
                    let open = self.picker == Some((f, pos));
                    let this = this.clone();
                    let (hue, sat, val, hex) = (hue.clone(), sat.clone(), val.clone(), hex.clone());
                    let palette = palette.clone();
                    let cell_id = f.saturating_mul(8).saturating_add(i);
                    div().flex_1().child(
                        Popover::new(("cell", cell_id))
                            .anchor(Anchor::TopRight)
                            .open(open)
                            .on_open_change({
                                let this = this.clone();
                                move |open: &bool, _, cx| {
                                    let open = *open;
                                    this.update(cx, |a, cx| {
                                        if open {
                                            a.open_picker(f, pos);
                                        } else if a.picker == Some((f, pos)) {
                                            a.picker = None;
                                        }
                                        cx.notify();
                                    })
                                    .ok();
                                }
                            })
                            .trigger(Cell { id: ("cell-btn", cell_id).into(), color: hex_hsla(&color), selected: open })
                            .content(move |_, _, cx| {
                                let muted = cx.theme().muted_foreground;
                                let this = this.clone();
                                v_flex()
                                    .w(px(240.))
                                    .gap_3()
                                    .text_xs()
                                    .child(
                                        v_flex()
                                            .gap_1()
                                            .child(div().text_color(muted).child("Hue"))
                                            .child(Slider::new(&hue)),
                                    )
                                    .child(
                                        v_flex()
                                            .gap_1()
                                            .child(div().text_color(muted).child("Saturation"))
                                            .child(Slider::new(&sat)),
                                    )
                                    .child(
                                        v_flex()
                                            .gap_1()
                                            .child(div().text_color(muted).child("Brightness"))
                                            .child(Slider::new(&val)),
                                    )
                                    .child(Input::new(&hex).small())
                                    .child(
                                        v_flex()
                                            .gap_1()
                                            .child(div().text_color(muted).child("Colours in this animation"))
                                            .child(h_flex().gap_1p5().children(palette.iter().enumerate().map(
                                                |(k, c)| {
                                                    let c2 = c.clone();
                                                    let this = this.clone();
                                                    div()
                                                        .id(("swatch", k))
                                                        .flex_1()
                                                        .h(px(18.))
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .bg(hex_hsla(c))
                                                        .on_click(move |_, _, cx| {
                                                            let c = c2.clone();
                                                            this.update(cx, |a, cx| {
                                                                if let Some((r, g, b)) = parse_hex(&c) {
                                                                    a.picker_hsv = rgb_to_hsv(r, g, b);
                                                                }
                                                                a.set_picked_color(&c);
                                                                cx.notify();
                                                            })
                                                            .ok();
                                                        })
                                                },
                                            ))),
                                    )
                            }),
                    )
                }))
        }));

        let frame_buttons = h_flex()
            .gap_1p5()
            .child(
                Button::new("add-frame")
                    .small()
                    .icon(IconName::Plus)
                    .label("Add frame")
                    .disabled(frames >= FRAMES)
                    .on_click(cx.listener(move |a, _, _, cx| {
                        a.edit(cx, |d| {
                            duplicate_frame(&mut d.led, frames.saturating_sub(1));
                            d.led.loop_end = d.led.loop_end.saturating_add(1);
                            d.led.app_preset = 0;
                        });
                        a.led_frame = frames;
                    })),
            )
            .child(Button::new("dup-frame").small().label("Duplicate").disabled(frames >= FRAMES).on_click(
                cx.listener(move |a, _, _, cx| {
                    a.edit(cx, |d| {
                        duplicate_frame(&mut d.led, selected);
                        d.led.loop_end = d.led.loop_end.saturating_add(1);
                        d.led.app_preset = 0;
                    });
                    a.led_frame = selected.saturating_add(1);
                }),
            ))
            .child(Button::new("rm-frame").small().label("Remove").disabled(frames <= 1).on_click(cx.listener(
                move |a, _, _, cx| {
                    a.picker = None;
                    a.edit(cx, |d| {
                        remove_frame(&mut d.led, selected);
                        d.led.loop_end = d.led.loop_end.saturating_sub(1);
                        d.led.app_preset = 0;
                    });
                    a.led_frame = selected.saturating_sub(1);
                },
            )));

        let preset = led.app_preset;
        let preset_button =
            Button::new("preset").small().w(px(170.)).label(preset_name(preset)).dropdown_caret(true).dropdown_menu({
                let this = this.clone();
                move |menu, _, _| {
                    PRESETS.iter().fold(menu, |menu, &(id, name)| {
                        let this = this.clone();
                        menu.item(PopupMenuItem::new(name).checked(id == preset).on_click(move |_, _, cx| {
                            this.update(cx, |a, cx| {
                                a.picker = None;
                                a.edit(cx, |d| {
                                    if id == 0 {
                                        d.led.app_preset = 0;
                                    } else {
                                        model::apply_preset(&mut d.led, id);
                                    }
                                });
                            })
                            .ok();
                        }))
                    })
                }
            });
        let preview_colors = LED_ORDER.map(|p| hex_hsla(led_color(led, selected, p)));
        let step_ms = u32::from(led.step_time).saturating_mul(100);
        let advanced_open = self.advanced_open("leds");

        h_flex()
            .items_start()
            .gap_4()
            .child(
                v_flex()
                    .w(px(440.))
                    .flex_shrink_0()
                    .gap_2()
                    .child(h_flex().justify_between().child(heading("Frames")).child(frame_buttons))
                    .child(card(cx).p_3().child(grid))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .px_0p5()
                            .child(format!("Plays frames 1–{frames}, {step_ms} ms each")),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(heading("Effect"))
                    .child(
                        card(cx)
                            .child(row("Preset", None, preset_button, cx).border_t_0())
                            .child(row(
                                "Plays",
                                None,
                                segmented(
                                    "led-mode",
                                    &["Continuously", "On button press"],
                                    Some(usize::from(led.feedback.min(1))),
                                    {
                                        let this = this.clone();
                                        move |k, _, cx| {
                                            this.update(cx, |a, cx| {
                                                a.edit(cx, |d| {
                                                    d.led.feedback = u8::try_from(k).unwrap_or(0);
                                                    d.led.app_preset = 0;
                                                });
                                            })
                                            .ok();
                                        }
                                    },
                                ),
                                cx,
                            ))
                            .child(slider_row("Brightness", &brightness, format!("{}%", led.brightness), false, cx))
                            .child(slider_row("Frame time", &step, format!("{step_ms} ms"), false, cx))
                            .child(self.advanced_toggle("leds".to_owned(), cx))
                            .child(
                                CollapsibleView::new().open(advanced_open).motion_id("adv-leds").content(
                                    v_flex()
                                        .child(subheading("Values the controller ignores", cx))
                                        .child(slider_row(
                                            "Loop start",
                                            &loop_start,
                                            led.loop_start.to_string(),
                                            false,
                                            cx,
                                        ))
                                        .child(slider_row(
                                            "LED count",
                                            &led_count,
                                            led.led_count.to_string(),
                                            false,
                                            cx,
                                        )),
                                ),
                            ),
                    )
                    .child(heading("Preview").mt_2())
                    .child(
                        card(cx).py_4().items_center().gap_2().child(preview(preview_colors)).child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("Frame {} of {frames}", selected.saturating_add(1))),
                        ),
                    ),
            )
            .into_any_element()
    }

    fn open_picker(&mut self, frame: usize, pos: LedPos) {
        self.picker = Some((frame, pos));
        self.led_frame = frame;
        if let Some((r, g, b)) = self.current().and_then(|d| parse_hex(led_color(&d.led, frame, pos))) {
            self.picker_hsv = rgb_to_hsv(r, g, b);
        }
    }

    fn set_picker_hsv(&mut self, f: impl FnOnce(&mut (f32, f32, f32))) {
        f(&mut self.picker_hsv);
        let color = hsv_hex(self.picker_hsv);
        self.set_picked_color(&color);
    }

    fn set_picked_color(&mut self, color: &str) {
        let Some((frame, pos)) = self.picker else { return };
        self.edit_silent(|d| {
            if led_color(&d.led, frame, pos) != color {
                set_led_color(&mut d.led, frame, pos, color);
                d.led.app_preset = 0;
            }
        });
    }
}
