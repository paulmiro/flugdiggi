//! The main window: controller/slot sidebar, page header and the page content.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::prelude::*;
use color_eyre::Result;
use flugdiggi_core::{DeviceSummary, Slot, UsbPort};
use gpui_kit::component::alert::Alert;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::progress::Progress;
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme, IconName, Sizable, TitleBar, WindowExt, h_flex, v_flex};
use gpui_kit::*;
use tracing::{error, info, warn};

use crate::device::{self, Loaded};
use crate::model::{LedPos, Page, SLOTS, SlotData};
use crate::sidebar;
use crate::theme::Colors;

/// A setting changed by a slider or text field: `(app, value)`.
pub type Setter<T> = Rc<dyn Fn(&mut FlugApp, T)>;
type TextSetter = Rc<dyn Fn(&mut FlugApp, &str)>;

struct Bound<S> {
    state: Entity<S>,
    _sub: Subscription,
}

pub struct Entry {
    pub summary: DeviceSummary,
    pub loaded: Option<Loaded>,
}

struct Busy {
    label: SharedString,
    progress: Option<(Arc<AtomicUsize>, usize)>,
}

#[derive(Clone)]
enum Nav {
    Slot(usize),
    Controller(UsbPort),
}

pub struct FlugApp {
    pub controllers: Vec<Entry>,
    pub selected: Option<UsbPort>,
    pub slot: usize,
    pub page: Page,
    busy: Option<Busy>,
    error: Option<SharedString>,
    /// Navigation to do once the pending save succeeded.
    after_save: Option<Nav>,
    sliders: HashMap<SharedString, Bound<SliderState>>,
    inputs: HashMap<SharedString, Bound<InputState>>,
    pub link_sticks: bool,
    pub link_triggers: bool,
    /// LED cell whose colour picker is open.
    pub picker: Option<(usize, LedPos)>,
    pub macro_sel: usize,
    /// Selected LED frame.
    pub led_frame: usize,
    /// Colour picker value as hue (degrees), saturation and value (0-1).
    pub picker_hsv: (f32, f32, f32),
    advanced: std::collections::HashSet<String>,
    _poll: Task<()>,
}

const POLL: Duration = Duration::from_secs(3);

impl FlugApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                let idle = this.update(cx, |this, _| this.busy.is_none()).unwrap_or(false);
                if idle {
                    let found = cx.background_spawn(async { device::list() }).await;
                    if this.update(cx, |this, cx| this.merge_list(found, cx)).is_err() {
                        break;
                    }
                }
                cx.background_executor().timer(POLL).await;
            }
        });
        window.on_window_should_close(cx, {
            let this = cx.entity().downgrade();
            move |window, cx| {
                let dirty = this.upgrade().is_some_and(|t| t.read(cx).current().is_some_and(SlotData::is_dirty));
                if dirty {
                    open_quit_dialog(window, cx);
                }
                !dirty
            }
        });
        FlugApp {
            controllers: Vec::new(),
            selected: None,
            slot: 0,
            page: Page::Buttons,
            busy: None,
            error: None,
            after_save: None,
            sliders: HashMap::new(),
            inputs: HashMap::new(),
            link_sticks: false,
            link_triggers: false,
            picker: None,
            macro_sel: 0,
            led_frame: 0,
            picker_hsv: (0.0, 0.0, 0.0),
            advanced: std::collections::HashSet::new(),
            _poll: poll,
        }
    }

    pub fn entry(&self) -> Option<&Entry> {
        let port = self.selected.as_ref()?;
        self.controllers.iter().find(|e| e.summary.port == *port)
    }

    fn entry_mut(&mut self) -> Option<&mut Entry> {
        let port = self.selected.clone()?;
        self.controllers.iter_mut().find(|e| e.summary.port == port)
    }

    pub fn loaded(&self) -> Option<&Loaded> {
        self.entry()?.loaded.as_ref()
    }

    pub fn current(&self) -> Option<&SlotData> {
        self.loaded()?.slots.get(self.slot)
    }

    pub fn is_busy(&self) -> bool {
        self.busy.is_some()
    }

    /// Changes the selected slot's unsaved edits. Ignored while the controller is busy.
    pub fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut SlotData)) {
        if self.busy.is_some() {
            return;
        }
        let slot = self.slot;
        if let Some(d) = self.entry_mut().and_then(|e| e.loaded.as_mut()).and_then(|l| l.slots.get_mut(slot)) {
            f(d);
            cx.notify();
        }
    }

    /// Like [`Self::edit`], for callers that redraw themselves.
    pub fn edit_silent(&mut self, f: impl FnOnce(&mut SlotData)) {
        if self.busy.is_some() {
            return;
        }
        let slot = self.slot;
        if let Some(d) = self.entry_mut().and_then(|e| e.loaded.as_mut()).and_then(|l| l.slots.get_mut(slot)) {
            f(d);
        }
    }

    fn merge_list(&mut self, found: Result<Vec<DeviceSummary>>, cx: &mut Context<Self>) {
        let found = match found {
            Ok(f) => f,
            Err(e) => {
                warn!("listing controllers failed: {e:#}");
                return;
            }
        };
        if self.busy.is_some() {
            return;
        }
        let mut old: Vec<Entry> = std::mem::take(&mut self.controllers);
        for summary in found {
            let loaded = old.iter().position(|e| e.summary.port == summary.port).and_then(|i| {
                let e = old.swap_remove(i);
                e.loaded
            });
            self.controllers.push(Entry { summary, loaded });
        }
        for gone in old {
            info!(port = %gone.summary.port, "controller disconnected");
        }
        if self.entry().is_none() {
            self.selected = None;
            if let Some(first) = self.controllers.first() {
                let port = first.summary.port.clone();
                self.select_controller(port, cx);
            }
        }
        cx.notify();
    }

    /// Runs `work` on a background thread, with the controller marked busy until `done` ran.
    fn run<T: Send + 'static>(
        &mut self,
        label: impl Into<SharedString>,
        progress: Option<(Arc<AtomicUsize>, usize)>,
        work: impl FnOnce() -> Result<T> + Send + 'static,
        done: impl FnOnce(&mut Self, Result<T>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let ticking = progress.is_some();
        self.busy = Some(Busy { label: label.into(), progress });
        self.error = None;
        let task = cx.background_spawn(async move { work() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.busy = None;
                done(this, result, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        if ticking {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(200)).await;
                    let busy = this
                        .update(cx, |this, cx| {
                            cx.notify();
                            this.busy.is_some()
                        })
                        .unwrap_or(false);
                    if !busy {
                        break;
                    }
                }
            })
            .detach();
        }
        cx.notify();
    }

    pub fn show_error(&mut self, what: &str, e: &color_eyre::Report, cx: &mut Context<Self>) {
        error!("{what}: {e:#}");
        self.error = Some(format!("{what}: {e:#}").into());
        cx.notify();
    }

    fn select_controller(&mut self, port: UsbPort, cx: &mut Context<Self>) {
        self.selected = Some(port);
        self.picker = None;
        cx.notify();
    }

    /// Looks for controllers now instead of waiting for the next poll.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let task = cx.background_spawn(async { device::list() });
        cx.spawn(async move |this, cx| {
            let found = task.await;
            this.update(cx, |this, cx| this.merge_list(found, cx)).ok();
        })
        .detach();
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(port) = self.selected.clone() else { return };
        let progress = Arc::new(AtomicUsize::new(0));
        let p = progress.clone();
        self.run(
            "Reading slots",
            Some((progress, usize::from(SLOTS))),
            move || device::load(&port, &p),
            |this, r, cx| match r {
                Ok(loaded) => {
                    let active = usize::from(loaded.active.slot.get());
                    if let Some(e) = this.entry_mut() {
                        e.loaded = Some(loaded);
                    }
                    if this.slot >= usize::from(SLOTS) {
                        this.slot = active;
                    }
                }
                Err(e) => this.show_error("Reading the controller failed", &e, cx),
            },
            cx,
        );
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        let (Some(port), Some(data)) = (self.selected.clone(), self.current().cloned()) else { return };
        let Ok(slot) = Slot::new(u8::try_from(self.slot).unwrap_or(u8::MAX)) else { return };
        let index = self.slot;
        self.run(
            format!("Saving slot {slot}"),
            None,
            move || device::save(&port, slot, &data),
            move |this, r, cx| match r {
                Ok((config, led)) => {
                    let mut target = None;
                    if let Some(d) =
                        this.entry_mut().and_then(|e| e.loaded.as_mut()).and_then(|l| l.slots.get_mut(index))
                    {
                        *d = SlotData::new(config, led);
                        target = Some(());
                    }
                    if target.is_some() {
                        info!("slot {slot} saved");
                    }
                    if let Some(nav) = this.after_save.take() {
                        this.apply_nav(nav, cx);
                    }
                }
                Err(e) => {
                    this.after_save = None;
                    this.show_error("Saving failed", &e, cx);
                }
            },
            cx,
        );
    }

    pub fn discard(&mut self, cx: &mut Context<Self>) {
        self.picker = None;
        self.edit(cx, SlotData::discard);
    }

    pub fn activate(&mut self, cx: &mut Context<Self>) {
        let Some(port) = self.selected.clone() else { return };
        let Ok(slot) = Slot::new(u8::try_from(self.slot).unwrap_or(u8::MAX)) else { return };
        self.run(
            format!("Activating slot {slot}"),
            None,
            move || device::activate(&port, slot),
            |this, r, cx| match r {
                Ok(active) => {
                    if let Some(l) = this.entry_mut().and_then(|e| e.loaded.as_mut()) {
                        l.active = active;
                    }
                }
                Err(e) => this.show_error("Activating the slot failed", &e, cx),
            },
            cx,
        );
    }

    fn navigate(&mut self, nav: Nav, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let dirty = self.current().is_some_and(SlotData::is_dirty);
        if !dirty {
            self.apply_nav(nav, cx);
            return;
        }
        let this = cx.entity().downgrade();
        let slot = self.slot;
        window.open_dialog(cx, move |dialog, _, _| {
            let (save, discard) = (this.clone(), this.clone());
            let (nav_save, nav_discard) = (nav.clone(), nav.clone());
            dialog
                .title("Unsaved changes")
                .child(format!("Slot {slot} has unsaved changes. Save them to the controller first?"))
                .footer(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap_2()
                        .child(Button::new("cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                        .child(Button::new("discard").label("Discard").on_click(move |_, window, cx| {
                            let nav = nav_discard.clone();
                            discard
                                .update(cx, |this, cx| {
                                    this.discard(cx);
                                    this.apply_nav(nav, cx);
                                })
                                .ok();
                            window.close_dialog(cx);
                        }))
                        .child(Button::new("save").primary().label("Save").on_click(move |_, window, cx| {
                            let nav = nav_save.clone();
                            save.update(cx, |this, cx| {
                                this.after_save = Some(nav);
                                this.save(cx);
                            })
                            .ok();
                            window.close_dialog(cx);
                        })),
                )
        });
    }

    fn apply_nav(&mut self, nav: Nav, cx: &mut Context<Self>) {
        self.picker = None;
        match nav {
            Nav::Slot(s) => self.slot = s,
            Nav::Controller(port) => self.select_controller(port, cx),
        }
        cx.notify();
    }

    pub fn go_slot(&mut self, slot: usize, window: &mut Window, cx: &mut Context<Self>) {
        if slot != self.slot {
            self.navigate(Nav::Slot(slot), window, cx);
        }
    }

    pub fn go_controller(&mut self, port: UsbPort, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.as_ref() != Some(&port) {
            self.navigate(Nav::Controller(port), window, cx);
        }
    }

    pub fn advanced_open(&self, key: &str) -> bool {
        self.advanced.contains(key)
    }

    /// The "Advanced" row that shows or hides settings the official app does not offer.
    pub fn advanced_toggle(&self, key: String, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let open = self.advanced.contains(&key);
        div().border_t_1().border_color(Colors::line()).child(
            Button::new(SharedString::from(format!("adv-{key}")))
                .ghost()
                .small()
                .w_full()
                .h(px(36.))
                .label("Advanced")
                .icon(if open { IconName::ChevronDown } else { IconName::ChevronRight })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.advanced.remove(&key) {
                        this.advanced.insert(key.clone());
                    }
                    cx.notify();
                })),
        )
    }

    /// A slider bound to a setting. `value` is the setting's current value; `set` stores a new one.
    pub fn slider(
        &mut self,
        key: impl Into<SharedString>,
        range: (f32, f32),
        value: f32,
        set: impl Fn(&mut FlugApp, f32) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SliderState> {
        let key = key.into();
        let state = if let Some(b) = self.sliders.get(&key) {
            b.state.clone()
        } else {
            {
                let state = cx.new(|_| SliderState::new().min(range.0).max(range.1).step(1.0).default_value(value));
                let set: Setter<f32> = Rc::new(set);
                let sub = cx.subscribe_in(&state, window, move |this, _, ev: &SliderEvent, _, cx| {
                    if let SliderEvent::Change(v) = ev
                        && this.busy.is_none()
                    {
                        set(this, v.start());
                        cx.notify();
                    }
                });
                self.sliders.insert(key, Bound { state: state.clone(), _sub: sub });
                state
            }
        };
        if (state.read(cx).value().start() - value).abs() > 0.01 {
            state.update(cx, |s, cx| s.set_value(value, window, cx));
        }
        state
    }

    /// A text field bound to a setting. Its text is replaced when `value` changes while it is not focused.
    pub fn input(
        &mut self,
        key: impl Into<SharedString>,
        value: &str,
        set: impl Fn(&mut FlugApp, &str) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let key = key.into();
        let state = if let Some(b) = self.inputs.get(&key) {
            b.state.clone()
        } else {
            {
                let v = value.to_owned();
                let state = cx.new(|cx| InputState::new(window, cx).default_value(v));
                let set: TextSetter = Rc::new(set);
                let sub = cx.subscribe_in(&state, window, move |this, state, ev: &InputEvent, _, cx| {
                    if matches!(ev, InputEvent::Change) && this.busy.is_none() {
                        let text = state.read(cx).value();
                        set(this, &text);
                        cx.notify();
                    }
                });
                self.inputs.insert(key, Bound { state: state.clone(), _sub: sub });
                state
            }
        };
        let (current, focused) = {
            let s = state.read(cx);
            (s.value(), s.focus_handle(cx).is_focused(window))
        };
        if !focused && current.as_ref() != value {
            let v = value.to_owned();
            state.update(cx, |s, cx| s.set_value(v, window, cx));
        }
        state
    }

    pub fn export(&mut self, cx: &mut Context<Self>) {
        let Some(data) = self.current().cloned() else { return };
        let name = if data.config.name.is_empty() { format!("slot{}", self.slot) } else { data.config.name.clone() };
        let dir = std::env::var_os("HOME").map_or_else(std::env::temp_dir, PathBuf::from);
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{name}.toml")));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = rx.await else { return };
            if let Err(e) = device::export(&path, &data) {
                this.update(cx, |this, cx| this.show_error("Export failed", &e, cx)).ok();
            }
        })
        .detach();
    }

    pub fn import(cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let file = device::import(&path);
            this.update(cx, |this, cx| match file {
                Ok(f) => this.edit(cx, |d| {
                    if let Some(mut c) = f.config {
                        c.proto_version = d.config.proto_version;
                        c.data_version = d.config.data_version;
                        d.config = c;
                    }
                    if let Some(l) = f.led {
                        d.led = l;
                    }
                }),
                Err(e) => this.show_error("Import failed", &e, cx),
            })
            .ok();
        })
        .detach();
    }

    pub fn open_backups(cx: &mut Context<Self>) {
        let dir = device::backup_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            warn!("cannot create {}: {e}", dir.display());
        }
        cx.open_with_system(&dir);
    }

    fn render_header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let changes = self.current().map_or(0, SlotData::changes);
        let busy = self.busy.as_ref().map(|b| b.label.clone());
        let selected = Page::ALL.iter().position(|p| *p == self.page).unwrap_or(0);
        let has_data = self.current().is_some();
        let this = cx.entity().downgrade();
        let status = match (&busy, changes) {
            (Some(label), _) => h_flex().gap_2().child(Spinner::new().small()).child(label.clone()).into_any_element(),
            (None, _) if !has_data => div().into_any_element(),
            (None, 0) => div().child("No unsaved changes").into_any_element(),
            (None, n) => h_flex()
                .gap_2()
                .child(div().size(px(6.)).rounded_full().bg(Colors::edited()))
                .child(if n == 1 { "1 unsaved change".to_owned() } else { format!("{n} unsaved changes") })
                .into_any_element(),
        };
        let close = cx.entity().downgrade();
        TitleBar::new()
            .on_close_window(move |_, window, cx| {
                let dirty = close.upgrade().is_some_and(|t| t.read(cx).current().is_some_and(SlotData::is_dirty));
                if dirty {
                    open_quit_dialog(window, cx);
                } else {
                    window.remove_window();
                }
            })
            .child(
                h_flex()
                    .flex_1()
                    .h_full()
                    .pr_2()
                    .gap_2()
                    .when(has_data, |d| {
                        d.child(
                            TabBar::new("pages")
                                .pill()
                                .small()
                                .selected_index(selected)
                                .children(Page::ALL.iter().map(|p| Tab::new().label(p.label())))
                                .on_click(cx.listener(|this, ix: &usize, _, cx| {
                                    if let Some(p) = Page::ALL.get(*ix) {
                                        this.page = *p;
                                        this.picker = None;
                                        cx.notify();
                                    }
                                })),
                        )
                    })
                    .child(div().flex_1())
                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child(status))
                    .when(has_data, |d| {
                        d.child(
                            Button::new("discard")
                                .label("Discard")
                                .small()
                                .disabled(changes == 0 || busy.is_some())
                                .on_click(cx.listener(|this, _, _, cx| this.discard(cx))),
                        )
                        .child(
                            Button::new("save")
                                .label("Save")
                                .primary()
                                .small()
                                .disabled(changes == 0 || busy.is_some())
                                .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                        )
                    })
                    .child(Button::new("menu").icon(IconName::Menu).ghost().small().dropdown_menu(
                        move |menu, _, _| {
                            let (reload, backups) = (this.clone(), this.clone());
                            menu.item(
                                PopupMenuItem::new("Read again from the controller").disabled(!has_data).on_click(
                                    move |_, _, cx| {
                                        reload.update(cx, Self::reload).ok();
                                    },
                                ),
                            )
                            .item(PopupMenuItem::new("Open backups folder").on_click(
                                move |_, _, cx| {
                                    backups.update(cx, |_, cx| Self::open_backups(cx)).ok();
                                },
                            ))
                        },
                    )),
            )
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Some(Busy { label, progress: Some((done, total)) }) = &self.busy
            && self.current().is_none()
        {
            let done = done.load(Ordering::Relaxed).min(*total);
            let pct = if *total == 0 {
                0.0
            } else {
                100.0 * f32::from(u8::try_from(done).unwrap_or(0)) / f32::from(u8::try_from(*total).unwrap_or(1))
            };
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_3()
                .child(div().child(format!("{label}… {done} of {total}")))
                .child(div().w(px(320.)).child(Progress::new("load").value(pct)))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("The controller is disconnected from games while this runs."),
                )
                .into_any_element();
        }
        if self.controllers.is_empty() {
            return empty_state(
                "No controller connected",
                "Connect the dongle or a USB cable. Controllers show up here automatically.",
                cx,
            );
        }
        if self.entry().is_none() {
            return empty_state("Select a controller", "Choose one in the sidebar.", cx);
        }
        if self.current().is_none() {
            let page = self.render_unloaded(cx);
            return div().id("page").size_full().overflow_y_scroll().px_5().py_4().child(page).into_any_element();
        }
        let page = match self.page {
            Page::Buttons => self.render_buttons(window, cx),
            Page::Sticks => self.render_sticks(window, cx),
            Page::Triggers => self.render_triggers(window, cx),
            Page::Leds => self.render_leds(window, cx),
            Page::Motion => self.render_motion(window, cx),
            Page::Macros => self.render_macros(window, cx),
            Page::Vibration => self.render_vibration(window, cx),
            Page::Settings => self.render_settings(window, cx),
        };
        div().id("page").size_full().overflow_y_scroll().px_5().py_4().child(page).into_any_element()
    }
}

fn empty_state(title: &str, text: &str, cx: &App) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_1()
        .child(div().font_semibold().child(title.to_owned()))
        .child(div().text_sm().text_color(cx.theme().muted_foreground).child(text.to_owned()))
        .into_any_element()
}

fn open_quit_dialog(window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, |dialog, _, _| {
        dialog
            .title("Unsaved changes")
            .child("The selected slot has unsaved changes. Quit anyway and lose them?")
            .footer(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(Button::new("cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
                    .child(Button::new("quit").danger().label("Quit without saving").on_click(|_, window, _| {
                        window.remove_window();
                    })),
            )
    });
}

impl Render for FlugApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let error = self.error.clone();
        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(sidebar::render(self, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.render_header(cx))
                    .when_some(error, |this, e| {
                        this.child(div().px_5().pt_3().child(Alert::error("error", e).on_close(cx.listener(
                            |this, _, _, cx| {
                                this.error = None;
                                cx.notify();
                            },
                        ))))
                    })
                    .child(div().flex_1().min_h_0().child(self.render_body(window, cx))),
            )
    }
}
