use std::time::{Duration, Instant};

use crate::prelude::*;
use flugdiggi_core::led::Led;
use gpui_kit::component::button::Button;
use gpui_kit::component::collapsible::Collapsible as CollapsibleView;
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState, ColorSelect};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable, h_flex, v_flex};
use gpui_kit::*;

use crate::app::FlugApp;
use crate::model::{
    self, FRAMES, LED_ORDER, LedPos, PRESETS, Page, frame_count, led_color, parse_hex, preset_name, set_led_color,
    unit_to_u8,
};
use crate::theme::Colors;
use crate::widgets::{byte, card, heading, row, segmented, slider_row, subheading};

pub fn hex_hsla(s: &str) -> Hsla {
    let (r, g, b) = parse_hex(s).unwrap_or((0, 0, 0));
    rgb(u32::from_be_bytes([0, r, g, b])).into()
}

fn hsla_hex(c: Hsla) -> String {
    let c = Rgba::from(c);
    let [r, g, b] = [c.r, c.g, c.b].map(|v| unit_to_u8(v * 255.0));
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn pos_name(pos: LedPos) -> &'static str {
    match pos {
        LedPos::TopLeft => "top left",
        LedPos::MiddleLeft => "middle left",
        LedPos::Bottom => "bottom",
        LedPos::MiddleRight => "middle right",
        LedPos::TopRight => "top right",
    }
}

/// Inserts a copy of `frame` after it; the last stored frame drops off.
fn duplicate_frame(led: &mut Led, frame: usize) {
    for pos in LED_ORDER {
        let steps = model::led_steps_mut(&mut led.leds, pos);
        if let Some(c) = steps.get(frame).cloned() {
            steps.insert(frame.saturating_add(1), c);
            steps.truncate(FRAMES);
        }
    }
}

fn remove_frame(led: &mut Led, frame: usize) {
    for pos in LED_ORDER {
        let steps = model::led_steps_mut(&mut led.leds, pos);
        if frame < steps.len() {
            steps.remove(frame);
            steps.push("#000000".to_owned());
        }
    }
}

/// A colour tile being dragged onto another.
#[derive(Clone)]
struct DraggedColor {
    color: String,
}

impl Render for DraggedColor {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size(px(28.)).rounded(px(6.)).border_2().border_color(rgb(0x00ec_ecec)).bg(hex_hsla(&self.color))
    }
}

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    let m = |x: f32, y: f32| x + (y - x) * t;
    Rgba { r: m(a.r, b.r), g: m(a.g, b.g), b: m(a.b, b.b), a: 1.0 }.into()
}

fn frame_colors(led: &Led, frame: usize) -> [Hsla; 5] {
    LED_ORDER.map(|p| hex_hsla(led_color(led, frame, p)))
}

/// The LEDs' colours `elapsed` into the animation, as the controller plays it: each step fades into the
/// next over the step time. Played continuously it loops back to the first step; played on a button press
/// it runs once and stays on the last step. Also returns whether it is still running.
fn animation_frame(led: &Led, elapsed: Duration) -> ([Hsla; 5], bool) {
    let frames = frame_count(led);
    let last = frames.saturating_sub(1);
    let step_ms = f32::from(led.step_time.max(1)) * 100.0;
    let pos = elapsed.as_secs_f32() * 1000.0 / step_ms;
    let count = f32::from(u8::try_from(frames).unwrap_or(1));
    let looping = led.feedback == 0;
    if !looping && pos >= count - 1.0 {
        return (frame_colors(led, last), false);
    }
    let pos = if looping { pos % count } else { pos };
    let k = usize::from(unit_to_u8(pos.floor())).min(last);
    let t = pos - pos.floor();
    let next = if k >= last { 0 } else { k.saturating_add(1) };
    let (a, b) = (frame_colors(led, k), frame_colors(led, next));
    (
        std::array::from_fn(|i| mix(a.get(i).copied().unwrap_or_default(), b.get(i).copied().unwrap_or_default(), t)),
        true,
    )
}

/// The LED strip as a V. Each LED's colour holds near it and fades into its neighbours'.
fn preview(colors: [Hsla; 5]) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            let (x0, y0) = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
            let (w, h) = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
            let half = 6.0;
            let (left, right, mid) = (x0 + w * 0.06, x0 + w * 0.94, x0 + w * 0.5);
            let (top, bottom) = (y0 + h * 0.12, y0 + h * 0.88);
            // Unit vector down the left arm; the right arm mirrors it.
            let len = (mid - left).hypot(bottom - top).max(f32::EPSILON);
            let (dx, dy) = ((mid - left) / len, (bottom - top) / len);
            let center = |t: f32| (left + (right - left) * t, top + (bottom - top) * (1.0 - (t - 0.5).abs() * 2.0));
            // A point on the band's outer (lower) or inner edge. At the tip both arms' edges meet on the
            // middle line, so the V has no gap there.
            let edge = |t: f32, outer: bool| {
                let s = if outer { half } else { -half };
                if (t - 0.5).abs() < 1e-4 {
                    let (ox, oy) = (left - dy * s, top + dx * s);
                    return point(px(mid), px(oy + (mid - ox) / dx * dy));
                }
                let (cx, cy) = center(t);
                let (nx, ny) = if t < 0.5 { (-dy, dx) } else { (dy, dx) };
                point(px(cx + nx * s), px(cy + ny * s))
            };
            let left_angle = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
            let right_angle = dx.atan2(dy).to_degrees().rem_euclid(360.0);
            for k in 0..4u8 {
                let (t0, t1) = (f32::from(k) / 4.0, f32::from(k.saturating_add(1)) / 4.0);
                let mut p = PathBuilder::fill();
                p.move_to(edge(t0, true));
                p.line_to(edge(t1, true));
                p.line_to(edge(t1, false));
                p.line_to(edge(t0, false));
                p.close();
                let a = colors.get(usize::from(k)).copied().unwrap_or_default();
                let b = colors.get(usize::from(k.saturating_add(1))).copied().unwrap_or(a);
                let angle = if k < 2 { left_angle } else { right_angle };
                let bg = linear_gradient(angle, linear_color_stop(a, 0.3), linear_color_stop(b, 0.7));
                if let Ok(path) = p.build() {
                    window.paint_path(path, bg);
                }
            }
        },
    )
    .w(px(360.))
    .h(px(120.))
}

impl FlugApp {
    fn led_color_state(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<ColorPickerState> {
        if let Some((state, _)) = &self.led_picker {
            return state.clone();
        }
        let state = cx.new(|cx| ColorPickerState::new(window, cx));
        let sub = cx.subscribe_in(&state, window, |this, _, ev: &ColorPickerEvent, _, cx| {
            if let ColorPickerEvent::Change(Some(c)) = ev {
                this.set_picked_color(&hsla_hex(*c));
                cx.notify();
            }
        });
        self.led_picker = Some((state.clone(), sub));
        state
    }

    pub fn render_leds(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.current().cloned() else { return div().into_any_element() };
        let led = &d.led;
        let frames = frame_count(led);
        let selected = self.led_frame.min(frames.saturating_sub(1));
        let color_state = self.led_color_state(window, cx);
        let wanted = self.picker.map(|(f, p)| hex_hsla(led_color(led, f, p)));
        color_state.update(cx, |s, cx| {
            if !s.is_open() && s.value() != wanted {
                match wanted {
                    Some(c) => s.set_value(c, window, cx),
                    None => s.clear_value(window, cx),
                }
            }
        });

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

        let mut palette: Vec<Hsla> = Vec::new();
        for f in 0..frames {
            for p in LED_ORDER {
                let c = hex_hsla(led_color(led, f, p));
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
                            a.led_anim = None;
                            cx.notify();
                        })),
                )
                .children(LED_ORDER.iter().enumerate().map(|(i, &pos)| {
                    let color = led_color(led, f, pos).to_owned();
                    let selected = self.picker == Some((f, pos));
                    let cell_id = f.saturating_mul(8).saturating_add(i);
                    div()
                        .id(("cell", cell_id))
                        .flex_1()
                        .h(px(32.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .bg(hex_hsla(&color))
                        .when(selected, |d| d.border_2().border_color(rgb(0x00ec_ecec)))
                        .drag_over::<DraggedColor>(|s, _, _, _| s.border_2().border_color(Colors::highlight()))
                        .on_drag(DraggedColor { color }, |d, _, _, cx| cx.new(|_| d.clone()))
                        .on_drop(cx.listener(move |a, dragged: &DraggedColor, _, cx| {
                            let c = dragged.color.clone();
                            a.edit(cx, |d| {
                                if led_color(&d.led, f, pos) != c {
                                    set_led_color(&mut d.led, f, pos, &c);
                                    d.led.app_preset = 0;
                                }
                            });
                        }))
                        .on_click(cx.listener(move |a, _, window, cx| a.open_picker(f, pos, window, cx)))
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

        let (preview_colors, playing) = match self.led_anim {
            Some(start) => {
                let (colors, running) = animation_frame(led, start.elapsed());
                self.led_anim_done = !running;
                (colors, running)
            }
            None => (frame_colors(led, selected), false),
        };
        let preview_text = match (self.led_anim, playing) {
            (Some(_), true) => "Playing".to_owned(),
            (Some(_), false) => format!("Played once, stays on frame {frames}"),
            (None, _) => format!("Frame {} of {frames}", selected.saturating_add(1)),
        };
        let play_button = Button::new("led-play")
            .small()
            .icon(Icon::new(if playing { gpui_kit::assets::IconName::Pause } else { gpui_kit::assets::IconName::Play }))
            .label(if playing { "Pause" } else { "Play" })
            .on_click(cx.listener(move |a, _, _, cx| {
                if playing {
                    a.led_anim = None;
                    cx.notify();
                } else {
                    a.play_leds(cx);
                }
            }));
        let step_ms = u32::from(led.step_time).saturating_mul(100);
        let advanced_open = self.advanced_open("leds");
        let picked_label = self.picker.map_or_else(
            || "Selected LED".to_owned(),
            |(f, p)| format!("Frame {}, {} LED", f.saturating_add(1), pos_name(p)),
        );

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
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).px_0p5().child(format!(
                        "Plays frames 1–{frames}, {step_ms} ms each. Click a tile to change its colour, \
                                 drag it onto another tile to copy it there."
                    ))),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(heading("Effect"))
                    .child(
                        card(cx)
                            .child(
                                row(picked_label, None, ColorSelect::new(&color_state).featured_colors(palette), cx)
                                    .border_t_0(),
                            )
                            .child(row("Preset", None, preset_button, cx))
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
                    .child(h_flex().mt_2().justify_between().child(heading("Preview")).child(play_button))
                    .child(
                        card(cx)
                            .py_4()
                            .items_center()
                            .gap_2()
                            .child(preview(preview_colors))
                            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(preview_text)),
                    ),
            )
            .into_any_element()
    }

    /// Starts the preview animation; it redraws until paused, the page is left or a play-once run ends.
    fn play_leds(&mut self, cx: &mut Context<Self>) {
        self.led_anim = Some(Instant::now());
        self.led_anim_done = false;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(33)).await;
                let go = this
                    .update(cx, |a, cx| {
                        if a.page != Page::Leds {
                            a.led_anim = None;
                        }
                        cx.notify();
                        a.led_anim.is_some() && !a.led_anim_done
                    })
                    .unwrap_or(false);
                if !go {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn open_picker(&mut self, frame: usize, pos: LedPos, window: &mut Window, cx: &mut Context<Self>) {
        self.picker = Some((frame, pos));
        self.led_frame = frame;
        self.led_anim = None;
        let color = self.current().map(|d| hex_hsla(led_color(&d.led, frame, pos)));
        let state = self.led_color_state(window, cx);
        state.update(cx, |s, cx| {
            if let Some(c) = color {
                s.set_value(c, window, cx);
            }
            s.set_open(true, cx);
        });
        cx.notify();
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
