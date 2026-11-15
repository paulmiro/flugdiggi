//! Controllers and their slots. Each slot shows the colours of its LEDs' first frame.

use std::rc::Rc;
use std::sync::Arc;

use crate::prelude::*;
use flugdiggi_core::{Battery, Connection};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarItem};
use gpui_kit::component::{ActiveTheme, Collapsible, Icon, IconName, Sizable, h_flex, v_flex};
use gpui_kit::*;

use crate::app::FlugApp;
use crate::model::{LED_ORDER, led_color, parse_hex};
use crate::theme::Colors;

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone)]
struct ControllerRow {
    name: SharedString,
    detail: SharedString,
    battery: Option<SharedString>,
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
    on_click: Handler,
}

#[derive(Clone)]
enum Rows {
    Controllers(Vec<ControllerRow>),
    Slots(Vec<SlotRow>),
}

impl Collapsible for Rows {
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

impl SidebarItem for Rows {
    fn render(self, id: impl Into<ElementId>, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let id: ElementId = id.into();
        let muted = cx.theme().muted_foreground;
        match self {
            Rows::Controllers(rows) => v_flex().gap_px().children(rows.into_iter().enumerate().map(|(i, r)| {
                row_base(
                    ElementId::NamedChild(Arc::new(id.clone()), format!("c{i}").into()),
                    r.selected,
                    r.on_click,
                    cx,
                )
                .child(
                    h_flex()
                        .gap_2p5()
                        .child(Icon::new(gpui_kit::assets::IconName::Gamepad2).small())
                        .child(
                            v_flex()
                                .flex_1()
                                .child(div().text_sm().font_medium().child(r.name))
                                .child(div().text_xs().text_color(muted).child(r.detail)),
                        )
                        .children(r.battery.map(|b| div().text_xs().text_color(muted).child(b))),
                )
                .into_any_element()
            })),
            Rows::Slots(rows) => {
                v_flex().gap_px().children(rows.into_iter().map(|r| {
                    let number_color = if r.selected { Colors::highlight() } else { muted };
                    row_base(
                        ElementId::NamedChild(Arc::new(id.clone()), format!("s{}", r.number).into()),
                        r.selected,
                        r.on_click,
                        cx,
                    )
                    .child(
                        v_flex()
                            .gap_1p5()
                            .child(
                                h_flex()
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
                                    .when(r.active, |d| {
                                        d.child(div().text_xs().font_semibold().text_color(muted).child("ACTIVE"))
                                    }),
                            )
                            .child(
                                h_flex().ml(px(30.)).h(px(4.)).gap(px(2.)).children(
                                    r.leds.into_iter().map(|c| div().flex_1().h_full().rounded(px(2.)).bg(c)),
                                ),
                            ),
                    )
                    .into_any_element()
                }))
            }
        }
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
            let battery = e.loaded.as_ref().map(|l| match l.info.battery {
                Battery::Level(n) => SharedString::from(format!("{n}/5")),
                Battery::Charging => "Charging".into(),
            });
            ControllerRow {
                name: controller_name(&s.product).into(),
                detail: format!("{connection} · {}", s.port).into(),
                battery,
                selected: app.selected.as_ref() == Some(&s.port),
                on_click: Rc::new(cx.listener(move |this, _, window, cx| this.go_controller(port.clone(), window, cx))),
            }
        })
        .collect();
    let mut groups = vec![SidebarGroup::new("Controllers").child(Rows::Controllers(controllers))];
    if let Some(loaded) = app.loaded() {
        let slot_row = |i: usize| {
            let d = loaded.slots.get(i)?;
            let number = u8::try_from(i).ok()?;
            let name = if d.config.name.trim().is_empty() { "Unnamed".to_owned() } else { d.config.name.clone() };
            Some(SlotRow {
                number,
                name: name.into(),
                leds: LED_ORDER.iter().map(|&p| hex_color(led_color(&d.led, 0, p))).collect(),
                selected: app.slot == i,
                active: loaded.active.slot.get() == number,
                edited: d.is_dirty(),
                on_click: Rc::new(cx.listener(move |this, _, window, cx| this.go_slot(i, window, cx))),
            })
        };
        groups.push(SidebarGroup::new("Slots").child(Rows::Slots((0..4).filter_map(slot_row).collect())));
        groups.push(SidebarGroup::new("Switch mode").child(Rows::Slots((4..8).filter_map(slot_row).collect())));
    }
    Sidebar::new("sidebar")
        .collapsible(false)
        .w(px(250.))
        .header(
            SidebarHeader::new().child(
                h_flex().w_full().justify_between().child(div().font_bold().text_sm().child("Flugdiggi")).child(
                    Button::new("refresh")
                        .icon(IconName::RefreshCw)
                        .ghost()
                        .small()
                        .tooltip("Look for controllers")
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                ),
            ),
        )
        .children(groups)
}
