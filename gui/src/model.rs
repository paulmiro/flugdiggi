//! Editable slot data and the controller-specific tables the pages are built from.

use flugdiggi_core::config::{Config, Macro, Turbo};
use flugdiggi_core::led::{Led, Leds};

/// Number of slots: 0-3 for `XInput` mode, 4-7 for Switch mode.
pub const SLOTS: u8 = 8;
/// Animation steps stored per LED.
pub const FRAMES: usize = 10;
pub const MAX_MACROS: usize = 5;
/// Pause between macro repeats the official app starts with.
const DEFAULT_PAUSE_MS: u16 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Buttons,
    Sticks,
    Triggers,
    Leds,
    Motion,
    Macros,
    Vibration,
    Settings,
}

impl Page {
    pub const ALL: [Page; 8] = [
        Page::Buttons,
        Page::Sticks,
        Page::Triggers,
        Page::Leds,
        Page::Motion,
        Page::Macros,
        Page::Vibration,
        Page::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Page::Buttons => "Buttons",
            Page::Sticks => "Sticks",
            Page::Triggers => "Triggers",
            Page::Leds => "LEDs",
            Page::Motion => "Motion",
            Page::Macros => "Macros",
            Page::Vibration => "Vibration",
            Page::Settings => "Settings",
        }
    }
}

/// The remappable buttons of the Vader 4 Pro, in the order the Buttons page lists them.
pub const BUTTONS: [&str; 22] = [
    "a", "b", "x", "y", "lb", "rb", "lt", "rt", "ls", "rs", "select", "start", "up", "down", "left", "right", "c", "z",
    "m1", "m2", "m3", "m4",
];

/// Buttons without an Xbox 360 equivalent; by default they send nothing.
pub fn is_extra(button: &str) -> bool {
    matches!(button, "c" | "z" | "m1" | "m2" | "m3" | "m4")
}

/// Targets a button can be mapped to.
pub const TARGETS: [&str; 17] =
    ["a", "b", "x", "y", "lb", "rb", "lt", "rt", "ls", "rs", "select", "start", "up", "down", "left", "right", "home"];

pub fn key_label(key: &str) -> String {
    match key {
        "select" => "Select".into(),
        "start" => "Start".into(),
        "up" | "stick_up" => "Up".into(),
        "down" | "stick_down" => "Down".into(),
        "left" | "stick_left" => "Left".into(),
        "right" | "stick_right" => "Right".into(),
        "home" => "Guide".into(),
        "fn" => "FN".into(),
        "none" => "Nothing".into(),
        "macro" => "Macro".into(),
        "stick_center" => "Center".into(),
        "stick_up_right" => "Up right".into(),
        "stick_down_right" => "Down right".into(),
        "stick_down_left" => "Down left".into(),
        "stick_up_left" => "Up left".into(),
        k => k.to_uppercase(),
    }
}

pub const STICK_DIRECTIONS: [&str; 9] = [
    "stick_center",
    "stick_up",
    "stick_up_right",
    "stick_right",
    "stick_down_right",
    "stick_down",
    "stick_down_left",
    "stick_left",
    "stick_up_left",
];

/// Macro step events (see `flugdiggi_core::config::Step`).
pub const EVENT_RELEASE: u8 = 0;
pub const EVENT_PRESS: u8 = 1;
pub const EVENT_LEFT_STICK: u8 = 2;
pub const EVENT_RIGHT_STICK: u8 = 3;

/// The five LEDs in the order of the V on the controller, left to right.
pub const LED_ORDER: [LedPos; 5] =
    [LedPos::TopLeft, LedPos::MiddleLeft, LedPos::Bottom, LedPos::MiddleRight, LedPos::TopRight];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedPos {
    TopLeft,
    MiddleLeft,
    Bottom,
    MiddleRight,
    TopRight,
}

pub fn led_steps(leds: &Leds, pos: LedPos) -> &Vec<String> {
    match pos {
        LedPos::TopLeft => &leds.top_left,
        LedPos::MiddleLeft => &leds.middle_left,
        LedPos::Bottom => &leds.bottom,
        LedPos::MiddleRight => &leds.middle_right,
        LedPos::TopRight => &leds.top_right,
    }
}

pub fn led_steps_mut(leds: &mut Leds, pos: LedPos) -> &mut Vec<String> {
    match pos {
        LedPos::TopLeft => &mut leds.top_left,
        LedPos::MiddleLeft => &mut leds.middle_left,
        LedPos::Bottom => &mut leds.bottom,
        LedPos::MiddleRight => &mut leds.middle_right,
        LedPos::TopRight => &mut leds.top_right,
    }
}

/// Colour of one LED in one frame, as `#rrggbb`.
pub fn led_color(led: &Led, frame: usize, pos: LedPos) -> &str {
    led_steps(&led.leds, pos).get(frame).map_or("#000000", String::as_str)
}

pub fn set_led_color(led: &mut Led, frame: usize, pos: LedPos, color: &str) {
    if let Some(c) = led_steps_mut(&mut led.leds, pos).get_mut(frame) {
        color.clone_into(c);
    }
}

/// Number of frames the animation plays (`0..=loop_end`).
pub fn frame_count(led: &Led) -> usize {
    usize::from(led.loop_end).saturating_add(1).min(FRAMES)
}

/// Presets of the official app, by their `app_preset` value (0 = custom).
pub const PRESETS: [(u8, &str); 7] = [
    (0, "Custom"),
    (1, "Flowing light"),
    (2, "Breathing"),
    (3, "Gradient"),
    (4, "Feedback"),
    (5, "Steady"),
    (6, "Off"),
];

pub fn preset_name(id: u8) -> &'static str {
    PRESETS.iter().find(|(i, _)| *i == id).map_or("Custom", |(_, n)| n)
}

fn fill(led: &mut Led, frames: &[[String; 5]]) {
    for (f, colors) in frames.iter().enumerate() {
        for (pos, c) in LED_ORDER.iter().zip(colors) {
            set_led_color(led, f, *pos, c);
        }
    }
    for f in frames.len()..FRAMES {
        for pos in LED_ORDER {
            set_led_color(led, f, pos, "#000000");
        }
    }
    let last = frames.len().saturating_sub(1);
    led.loop_end = u8::try_from(last).unwrap_or(0);
}

fn hue_color(hue: f32) -> String {
    let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Rewrites the frames for one of the official app's presets, based on the current first frame.
pub fn apply_preset(led: &mut Led, preset: u8) {
    let first: [String; 5] = LED_ORDER.map(|p| led_color(led, 0, p).to_owned());
    let black = || "#000000".to_owned();
    let main = first.iter().find(|c| c.as_str() != "#000000").cloned().unwrap_or_else(|| "#2251e0".to_owned());
    led.app_preset = preset;
    led.feedback = 0;
    match preset {
        1 => {
            let frames: Vec<[String; 5]> = (0..5usize)
                .map(|f| std::array::from_fn(|i| first.get(i.wrapping_add(f) % 5).cloned().unwrap_or_else(black)))
                .collect();
            fill(led, &frames);
        }
        2 => fill(led, &[std::array::from_fn(|_| main.clone()), std::array::from_fn(|_| black())]),
        3 => {
            let frames: Vec<[String; 5]> = [0.0, 60.0, 120.0, 180.0, 240.0, 300.0]
                .into_iter()
                .map(|h| std::array::from_fn(|_| hue_color(h)))
                .collect();
            fill(led, &frames);
        }
        4 => {
            fill(led, &[first.clone(), std::array::from_fn(|_| black())]);
            led.feedback = 1;
        }
        5 => fill(led, &[first]),
        6 => fill(led, &[std::array::from_fn(|_| black())]),
        _ => {}
    }
}

pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    let [_, r, g, b] = v.to_be_bytes();
    Some((r, g, b))
}

/// Converts a float in `0.0..=255.0` to a byte, clamping out-of-range values.
pub fn unit_to_u8(v: f32) -> u8 {
    let v = v.round().clamp(0.0, 255.0);
    #[allow(clippy::as_conversions, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = v as u8;
    b
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h {
        h if h < 1.0 => (c, x, 0.0),
        h if h < 2.0 => (x, c, 0.0),
        h if h < 3.0 => (0.0, c, x),
        h if h < 4.0 => (0.0, x, c),
        h if h < 5.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (unit_to_u8((r + m) * 255.0), unit_to_u8((g + m) * 255.0), unit_to_u8((b + m) * 255.0))
}

pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= f32::EPSILON {
        0.0
    } else if (max - r).abs() <= f32::EPSILON {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() <= f32::EPSILON {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max <= f32::EPSILON { 0.0 } else { d / max };
    (h, s, max)
}

/// A slot as read from the controller, plus the user's unsaved edits.
#[derive(Debug, Clone)]
pub struct SlotData {
    pub saved_config: Config,
    pub saved_led: Led,
    pub config: Config,
    pub led: Led,
}

impl SlotData {
    pub fn new(config: Config, led: Led) -> Self {
        SlotData { saved_config: config.clone(), saved_led: led.clone(), config, led }
    }

    pub fn discard(&mut self) {
        self.config.clone_from(&self.saved_config);
        self.led.clone_from(&self.saved_led);
    }

    pub fn is_dirty(&self) -> bool {
        self.config != self.saved_config || self.led != self.saved_led
    }

    /// Number of edited settings, as shown in the header.
    pub fn changes(&self) -> usize {
        let (a, b) = (&self.saved_config, &self.config);
        let mut n = 0usize;
        let mut count = |changed: bool| {
            if changed {
                n = n.saturating_add(1);
            }
        };
        count(a.name != b.name);
        for button in BUTTONS {
            count(a.buttons.get(button) != b.buttons.get(button) || a.turbo.get(button) != b.turbo.get(button));
        }
        count(a.left_stick != b.left_stick);
        count(a.right_stick != b.right_stick);
        count(a.left_trigger != b.left_trigger);
        count(a.right_trigger != b.right_trigger);
        count(a.rumble.enabled != b.rumble.enabled);
        count(a.rumble.left != b.rumble.left);
        count(a.rumble.right != b.rumble.right);
        count(a.trigger_rumble != b.trigger_rumble);
        count(a.motion != b.motion);
        let macros = a.macros.list.len().max(b.macros.list.len());
        for i in 0..macros {
            count(a.macros.list.get(i) != b.macros.list.get(i));
        }
        count(a.macros.intervals_ms != b.macros.intervals_ms);
        count(a.trigger_adapter != b.trigger_adapter);
        let (la, lb) = (&self.saved_led, &self.led);
        count(la.feedback != lb.feedback);
        count(la.loop_end != lb.loop_end);
        count(la.step_time != lb.step_time);
        count(la.brightness != lb.brightness);
        count(la.app_preset != lb.app_preset);
        count(la.loop_start != lb.loop_start || la.led_count != lb.led_count);
        for f in 0..FRAMES {
            count(LED_ORDER.iter().any(|&p| led_color(la, f, p) != led_color(lb, f, p)));
        }
        n
    }
}

pub fn turbo_of<'a>(config: &'a Config, button: &str) -> Option<&'a Turbo> {
    config.turbo.get(button).filter(|t| t.mode != "off")
}

pub fn macro_index(config: &Config, button: &str) -> Option<usize> {
    config.macros.list.iter().position(|m| m.button == button)
}

/// Sets or removes a button's mapping (`None` = acts as itself).
pub fn set_mapping(config: &mut Config, button: &str, target: Option<&str>) {
    config.buttons.0.retain(|(n, _)| n != button);
    if let Some(t) = target.filter(|t| *t != button) {
        config.buttons.set(button, t.to_owned());
    }
}

pub fn set_turbo(config: &mut Config, button: &str, turbo: Option<Turbo>) {
    config.turbo.0.retain(|(n, _)| n != button);
    if let Some(t) = turbo {
        config.turbo.set(button, t);
    }
}

/// Adds a macro on the first button that has none; returns its index.
pub fn add_macro(config: &mut Config) -> Option<usize> {
    if config.macros.list.len() >= MAX_MACROS {
        return None;
    }
    let free = |b: &&&str| macro_index(config, b).is_none();
    let unused = |b: &&&str| config.buttons.get(b).is_none() && config.turbo.get(b).is_none();
    let button = BUTTONS.iter().rev().find(|b| free(b) && unused(b)).or_else(|| BUTTONS.iter().rev().find(free))?;
    set_mapping(config, button, None);
    set_turbo(config, button, None);
    config.macros.list.push(Macro { button: (*button).to_owned(), mode: 1, steps: Vec::new() });
    Some(config.macros.list.len().saturating_sub(1))
}

/// Adds an empty macro on `button`; returns its index.
pub fn add_macro_on(config: &mut Config, button: &str) -> Option<usize> {
    if config.macros.list.len() >= MAX_MACROS || macro_index(config, button).is_some() {
        return None;
    }
    set_mapping(config, button, None);
    set_turbo(config, button, None);
    config.macros.list.push(Macro { button: button.to_owned(), mode: 1, steps: Vec::new() });
    Some(config.macros.list.len().saturating_sub(1))
}

pub fn remove_macro(config: &mut Config, index: usize) {
    if index < config.macros.list.len() {
        let m = config.macros.list.remove(index);
        set_mapping(config, &m.button, None);
        // The pause table is positional, so the later macros' pauses move up with them.
        if index < config.macros.intervals_ms.len() {
            config.macros.intervals_ms.remove(index);
            config.macros.intervals_ms.push(DEFAULT_PAUSE_MS);
        }
    }
}

/// Moves a macro to another button; the old button acts as itself again.
pub fn set_macro_button(config: &mut Config, index: usize, button: &str) {
    if macro_index(config, button).is_some() {
        return;
    }
    let Some(old) = config.macros.list.get(index).map(|m| m.button.clone()) else { return };
    set_mapping(config, &old, None);
    set_mapping(config, button, None);
    set_turbo(config, button, None);
    if let Some(m) = config.macros.list.get_mut(index) {
        button.clone_into(&mut m.button);
    }
}
