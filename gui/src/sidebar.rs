//! Controllers and their slots. Each slot shows the colours of its LEDs' first frame.

use std::rc::Rc;
use std::sync::Arc;

use crate::prelude::*;
use flugdiggi_core::{Battery, Connection};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::radio::Radio;
use gpui_kit::component::sidebar::{Sidebar, SidebarItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Collapsible, Icon, IconName, Sizable, h_flex, v_flex};
use gpui_kit::*;

use crate::app::FlugApp;
use crate::model::{LED_ORDER, SlotData, display_name, led_color, parse_hex};
use crate::theme::Colors;

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type Activate = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone)]
struct ControllerRow {
    name: SharedString,
    detail: SharedString,
    battery: Option<Battery>,
    selected: bool,
    on_click: Handler,
}

#[derive(Clone)]
struct SlotRow {
    number: u8,
    name: SharedString,
    leds: Vec<Hsla>,
    selected: bool,
    active: bool,
    edited: bool,
    /// `None` while the slot isn't loaded.
    on_click: Option<Handler>,
    /// Makes the controller use this slot, and whether that is possible right now; `None` where it isn't offered.
    on_activate: Option<(Activate, bool)>,
}

#[derive(Clone)]
enum Rows {
    Controllers(Vec<ControllerRow>),
    Slots(Vec<SlotRow>),
}

/// A labelled group of rows, optionally with a button next to the label.
#[derive(Clone)]
struct Group {
    label: SharedString,
    /// Icon, label, tooltip and click handler.
    action: Option<(IconName, Option<&'static str>, &'static str, Handler)>,
    rows: Rows,
}

impl Collapsible for Group {
    fn collapsed(self, _: bool) -> Self {
        self
    }

    fn is_collapsed(&self) -> bool {
        false
    }
}

fn row_base(id: ElementId, selected: bool, on_click: Handler, cx: &App) -> Stateful<Div> {
    div()
        .id(id)
        .w_full()
        .px_2p5()
        .py_1p5()
        .rounded(cx.theme().radius)
        .cursor_pointer()
        .when(selected, |d| d.bg(cx.theme().sidebar_accent))
        .when(!selected, |d| d.hover(|d| d.bg(cx.theme().sidebar_accent.opacity(0.5))))
        .on_click(move |e, w, cx| on_click(e, w, cx))
}

/// Icon, colour and hover text for a battery reading. The percentages are an assumption: the
/// controller only reports levels 0-5.
fn battery_view(b: Battery, cx: &App) -> (gpui_kit::assets::IconName, Option<Hsla>, String) {
    use gpui_kit::assets::IconName as I;
    let danger = Some(cx.theme().danger);
    match b {
        Battery::Charging => (I::BatteryCharging, None, "Charging".to_owned()),
        Battery::Level(n) => {
            let (icon, color, range) = match n {
                5.. => (I::BatteryFull, None, "about 80–100 %"),
                4 => (I::BatteryMedium, None, "about 60–80 %"),
                3 => (I::BatteryMedium, None, "about 40–60 %"),
                2 => (I::BatteryLow, None, "about 20–40 %"),
                1 => (I::BatteryLow, danger, "below about 20 %"),
                0 => (I::Battery, danger, "almost empty"),
            };
            (icon, color, format!("Battery level {n} of 5 ({range})"))
        }
    }
}

impl Rows {
    fn render(self, id: &ElementId, cx: &App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let child_id = |name: String| ElementId::NamedChild(Arc::new(id.clone()), name.into());
        match self {
            Rows::Controllers(rows) => v_flex()
                .gap_px()
                .children(rows.into_iter().enumerate().map(|(i, r)| {
                    let battery = r.battery.map(|b| {
                        let (icon, color, text) = battery_view(b, cx);
                        let text = SharedString::from(text);
                        div()
                            .id(child_id(format!("bat{i}")))
                            .text_color(color.unwrap_or(muted))
                            .child(Icon::new(icon).size_5())
                            .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
                    });
                    row_base(child_id(format!("c{i}")), r.selected, r.on_click, cx).child(
                        h_flex()
                            .gap_2p5()
                            .child(Icon::new(gpui_kit::assets::IconName::Gamepad2).small())
                            .child(
                                v_flex()
                                    .flex_1()
                                    .child(div().text_sm().font_medium().child(r.name))
                                    .child(div().text_xs().text_color(muted).child(r.detail)),
                            )
                            .children(battery),
                    )
                }))
                .into_any_element(),
            Rows::Slots(rows) => v_flex()
                .gap_px()
                .children(rows.into_iter().map(|r| {
                    let number_color = if r.selected { Colors::highlight() } else { muted };
                    let active = r.active;
                    let activate = r.on_activate.map(|(on_activate, enabled)| {
                        Radio::new(child_id(format!("act{}", r.number)))
                            .checked(active)
                            .disabled(!enabled)
                            .tooltip(if active {
                                "The controller uses this slot"
                            } else {
                                "Make the controller use this slot now"
                            })
                            .on_click(move |_, w, cx| {
                                if !active {
                                    on_activate(w, cx);
                                }
                            })
                    });
                    let has_radio = activate.is_some();
                    // The radio sits beside the row rather than inside it, so the two click targets don't overlap.
                    // Rows without one keep the space, so all slot rows line up.
                    let id = child_id(format!("s{}", r.number));
                    let row = match r.on_click {
                        Some(on_click) => row_base(id, r.selected, on_click, cx),
                        None => div().id(id).w_full().px_2p5().py_1p5().opacity(0.4),
                    };
                    let row = row.child(
                        v_flex()
                            .gap_1p5()
                            .child(
                                h_flex()
                                    .h(px(20.))
                                    .gap_2p5()
                                    .text_sm()
                                    .child(
                                        div()
                                            .w_5()
                                            .text_center()
                                            .font_semibold()
                                            .text_color(number_color)
                                            .child(r.number.to_string()),
                                    )
                                    .child(div().flex_1().child(r.name))
                                    .when(r.edited, |d| d.child(div().size(px(6.)).rounded_full().bg(Colors::edited())))
                                    .when(r.active && !has_radio, |d| {
                                        d.child(div().text_xs().font_semibold().text_color(muted).child("ACTIVE"))
                                    }),
                            )
                            .child(
                                h_flex().ml(px(30.)).h(px(4.)).gap(px(2.)).children(
                                    r.leds.into_iter().map(|c| div().flex_1().h_full().rounded(px(2.)).bg(c)),
                                ),
                            ),
                    );
                    h_flex()
                        .gap_1()
                        .child(div().flex_1().min_w_0().child(row))
                        .child(div().w(px(22.)).flex().justify_center().children(activate))
                }))
                .into_any_element(),
        }
    }
}

impl SidebarItem for Group {
    fn render(self, id: impl Into<ElementId>, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let id: ElementId = id.into();
        let action = self.action.map(|(icon, label, tooltip, on_click)| {
            Button::new(ElementId::NamedChild(Arc::new(id.clone()), "action".into()))
                .icon(icon)
                .when_some(label, Button::label)
                .ghost()
                .xsmall()
                .tooltip(tooltip)
                .on_click(move |e, w, cx| on_click(e, w, cx))
        });
        v_flex()
            .child(
                h_flex()
                    .h_8()
                    .px_2()
                    .justify_between()
                    .text_xs()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.7))
                    .child(self.label)
                    .children(action),
            )
            .child(self.rows.render(&id, cx))
    }
}

fn hex_color(s: &str) -> Hsla {
    match parse_hex(s) {
        Some((0, 0, 0)) | None => Colors::line(),
        Some((r, g, b)) => rgb(u32::from_be_bytes([0, r, g, b])).into(),
    }
}

fn controller_name(product: &str) -> String {
    let name = product.trim().trim_start_matches("Flydigi").trim();
    match name.to_ascii_uppercase().as_str() {
        "VADER4" => "Vader 4".to_owned(),
        _ if name.is_empty() => "Flydigi controller".to_owned(),
        _ => name.to_owned(),
    }
}

pub fn render(app: &FlugApp, cx: &mut Context<FlugApp>) -> impl IntoElement {
    let controllers: Vec<ControllerRow> = app
        .controllers
        .iter()
        .map(|e| {
            let s = &e.summary;
            let port = s.port.clone();
            let connection = e.loaded.as_ref().map_or(s.connection, |l| l.info.connection);
            let connection = match connection {
                Connection::Dongle => "Dongle",
                Connection::Cable => "Cable",
            };
            ControllerRow {
                name: controller_name(&s.product).into(),
                detail: format!("{connection} · {}", s.port).into(),
                battery: e.loaded.as_ref().map(|l| l.info.battery),
                selected: app.selected.as_ref() == Some(&s.port),
                on_click: Rc::new(cx.listener(move |this, _, window, cx| this.go_controller(port.clone(), window, cx))),
            }
        })
        .collect();
    let refresh: Handler = Rc::new(cx.listener(|this, _, _, cx| this.refresh(cx)));
    let mut groups = vec![Group {
        label: "Controllers".into(),
        action: Some((IconName::RefreshCw, None, "Look for controllers", refresh)),
        rows: Rows::Controllers(controllers),
    }];
    if let Some(loaded) = app.loaded() {
        let busy = app.is_busy();
        let slot_row = |i: usize| {
            let d = loaded.slots.get(i)?.as_ref();
            let number = u8::try_from(i).ok()?;
            let name = match d {
                None => "Not loaded".to_owned(),
                Some(d) if d.config.name.trim().is_empty() => "Unnamed".to_owned(),
                Some(d) => display_name(&d.config.name).to_owned(),
            };
            let active = loaded.active.slot.get() == number;
            let on_activate: Option<(Activate, bool)> = (i < 4).then(|| {
                let this = cx.entity().downgrade();
                let h: Activate = Rc::new(move |window, cx| {
                    this.update(cx, |a, cx| a.request_activate(i, window, cx)).ok();
                });
                (h, !busy)
            });
            Some(SlotRow {
                number,
                name: name.into(),
                leds: LED_ORDER
                    .iter()
                    .map(|&p| d.map_or_else(Colors::line, |d| hex_color(led_color(&d.led, 0, p))))
                    .collect(),
                selected: app.slot == i,
                active,
                edited: d.is_some_and(SlotData::is_dirty),
                on_click: d.is_some().then(|| -> Handler {
                    Rc::new(cx.listener(move |this, _, window, cx| this.go_slot(i, window, cx)))
                }),
                on_activate,
            })
        };
        groups.push(Group {
            label: "Slots".into(),
            action: None,
            rows: Rows::Slots((0..4).filter_map(slot_row).collect()),
        });
        let load_switch = (!app.switch_slots_loaded() && !busy).then(|| {
            let h: Handler = Rc::new(cx.listener(|this, _, _, cx| this.load_switch_slots(cx)));
            (
                IconName::ArrowDown,
                Some("Load"),
                "Read the Switch mode slots. The controller is disconnected from games for a few seconds.",
                h,
            )
        });
        groups.push(Group {
            label: "Switch mode slots".into(),
            action: load_switch,
            rows: Rows::Slots((4..8).filter_map(slot_row).collect()),
        });
    }
    Sidebar::new("sidebar")
        .collapsible(false)
        .w(px(250.))
        .header(
            div().id("sidebar-title").w_full().px_2().py_1().font_bold().text_sm().child("Flugdiggi").on_mouse_down(
                MouseButton::Left,
                |ev, window, _| {
                    if ev.click_count >= 2 {
                        window.zoom_window();
                    } else {
                        window.start_window_move();
                    }
                },
            ),
        )
        .children(groups)
}
