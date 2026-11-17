//! Editable slot data and the controller-specific tables the pages are built from.

use flugdiggi_core::config::{Config, Macro, Step, Turbo, is_parked_macro_button, parked_macro_button};
use flugdiggi_core::led::{Led, Leds};

/// Number of slots: 0-3 for `XInput` mode, 4-7 for Switch mode.
pub const SLOTS: u8 = 8;
/// Slots 0-3, used in `XInput` mode.
pub const XINPUT_SLOTS: u8 = 4;
/// Animation steps stored per LED.
pub const FRAMES: usize = 10;
pub const MAX_MACROS: usize = 5;
/// Pause between macro repeats the official app starts with.
const DEFAULT_PAUSE_MS: u16 = 30;
/// Size of the macro block in the config (bytes 230-767) and of its header (count and offsets).
const MACRO_BLOCK_BYTES: usize = 538;
const MACRO_HEADER_BYTES: usize = 6;

/// Left or right stick or trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Side::Left => 0,
            Side::Right => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Buttons,
    Macros,
    Sticks,
    Triggers,
    Leds,
    Motion,
    Vibration,
    Settings,
}

impl Page {
    pub const ALL: [Page; 8] = [
        Page::Buttons,
        Page::Macros,
        Page::Sticks,
        Page::Triggers,
        Page::Leds,
        Page::Motion,
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

/// English names the official app shows for the factory configs, keyed by the name stored on the controller.
const FACTORY_NAMES: [(&str, &str); 4] = [
    ("常规游戏配置", "Regular"),
    ("枪战游戏配置", "Shooting"),
    ("赛车游戏配置", "Racing"),
    ("格斗游戏配置", "Fighting"),
];

/// The name to show for a slot. Factory names are translated; the stored name is not changed.
pub fn display_name(name: &str) -> &str {
    FACTORY_NAMES.iter().find(|(stored, _)| *stored == name).map_or(name, |(_, shown)| shown)
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

/// The button macro `index` is applied to. A macro parked on a button the user can't see, or on one of
/// another model, counts as not applied.
pub fn macro_button(config: &Config, index: usize) -> Option<&str> {
    let m = config.macros.list.get(index)?;
    BUTTONS.contains(&m.button.as_str()).then_some(m.button.as_str())
}

pub fn macro_on_button(config: &Config, button: &str) -> Option<usize> {
    (0..MAX_MACROS).find(|&i| macro_button(config, i) == Some(button))
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

/// Makes sure records 0..=`index` exist (new ones are empty and parked) and returns record `index`.
pub fn ensure_macro(config: &mut Config, index: usize) -> Option<&mut Macro> {
    if index >= MAX_MACROS {
        return None;
    }
    while config.macros.list.len() <= index {
        let button = parked_macro_button(config.macros.list.len())?;
        config.macros.list.push(Macro { button, mode: 1, steps: Vec::new() });
    }
    config.macros.list.get_mut(index)
}

/// Drops trailing records that hold nothing, so unused macros take no space on the controller.
pub fn trim_macros(config: &mut Config) {
    while config.macros.list.last().is_some_and(|m| m.steps.is_empty() && is_parked_macro_button(&m.button)) {
        config.macros.list.pop();
    }
}

/// Edits macro `index`, creating its record if needed.
pub fn edit_macro(config: &mut Config, index: usize, f: impl FnOnce(&mut Macro)) {
    if let Some(m) = ensure_macro(config, index) {
        f(m);
    }
    trim_macros(config);
}

/// Takes macro `index` off its button, which then acts as itself again. The steps stay.
pub fn unapply_macro(config: &mut Config, index: usize) {
    let Some(old) = config.macros.list.get(index).map(|m| m.button.clone()) else { return };
    if is_parked_macro_button(&old) {
        return;
    }
    set_mapping(config, &old, None);
    if let (Some(parked), Some(m)) = (parked_macro_button(index), config.macros.list.get_mut(index)) {
        m.button = parked;
    }
}

/// Applies macro `index` to `button`. A macro already on `button` is taken off it, and the macro's
/// previous button acts as itself again.
pub fn apply_macro(config: &mut Config, index: usize, button: &str) {
    if macro_button(config, index) == Some(button) {
        return;
    }
    if let Some(other) = macro_on_button(config, button) {
        unapply_macro(config, other);
    }
    if ensure_macro(config, index).is_none() {
        return;
    }
    unapply_macro(config, index);
    set_mapping(config, button, None);
    set_turbo(config, button, None);
    if let Some(m) = config.macros.list.get_mut(index) {
        button.clone_into(&mut m.button);
    }
    trim_macros(config);
}

/// Empties macro `index`. It stays on its button.
pub fn clear_macro(config: &mut Config, index: usize) {
    if let Some(m) = config.macros.list.get_mut(index) {
        m.steps.clear();
        m.mode = 1;
    }
    if let Some(p) = config.macros.intervals_ms.get_mut(index) {
        *p = DEFAULT_PAUSE_MS;
    }
    trim_macros(config);
}

/// Steps that can still be added before the macro block of the config is full.
pub fn macro_steps_free(config: &Config, index: usize) -> usize {
    let records = config.macros.list.len().max(index.saturating_add(1));
    let steps: usize = config.macros.list.iter().map(|m| m.steps.len()).sum();
    let used = steps.saturating_add(records).saturating_mul(4).saturating_add(MACRO_HEADER_BYTES);
    MACRO_BLOCK_BYTES.saturating_sub(used) / 4
}

/// Latest time a macro step can have (stored in units of 10 ms in 16 bits).
const MAX_STEP_MS: u32 = 655_350;
/// Wait the official app puts before a step added by hand.
const DEFAULT_WAIT_MS: u32 = 100;
/// Values the wait sliders snap to.
pub const WAIT_SNAPS: [u32; 18] =
    [0, 10, 20, 30, 50, 80, 100, 150, 200, 300, 500, 750, 1000, 1500, 2000, 3000, 5000, 8000];

/// Slider stops for the pause between repeats (the official app allows up to 2500 ms).
pub const PAUSE_SNAPS: [u32; 16] = [0, 10, 20, 30, 50, 80, 100, 150, 200, 300, 500, 750, 1000, 1500, 2000, 2500];

/// Index of the value in `snaps` closest to `ms`.
pub fn nearest_snap(snaps: &[u32], ms: u32) -> usize {
    snaps.iter().enumerate().min_by_key(|(_, s)| s.abs_diff(ms)).map_or(0, |(i, _)| i)
}

/// Hue of a stick direction: a rainbow going clockwise from red at Up. `None` for the centre.
pub fn stick_direction_hue(key: &str) -> Option<f32> {
    let i = STICK_DIRECTIONS.iter().position(|d| *d == key).filter(|i| *i > 0)?;
    Some(f32::from(u8::try_from(i.saturating_sub(1)).ok()?) / 8.0)
}

/// Each step's wait after the previous step; the first step's wait counts from the button press.
pub fn step_waits(m: &Macro) -> Vec<u32> {
    let mut prev = 0u32;
    m.steps
        .iter()
        .map(|s| {
            let wait = s.at_ms.saturating_sub(prev);
            prev = s.at_ms;
            wait
        })
        .collect()
}

/// Sets the steps' times from their waits, in list order.
fn retime(m: &mut Macro, waits: &[u32]) {
    let mut t = 0u32;
    for (s, w) in m.steps.iter_mut().zip(waits) {
        t = t.saturating_add(*w).min(MAX_STEP_MS);
        s.at_ms = t;
    }
}

/// Changes how long step `index` waits after the previous one; later steps move with it.
pub fn set_step_wait(m: &mut Macro, index: usize, ms: u32) {
    let mut waits = step_waits(m);
    if let Some(w) = waits.get_mut(index) {
        *w = ms.saturating_div(10).saturating_mul(10);
    }
    retime(m, &waits);
}

/// Moves step `from` to position `to`, keeping every step's wait.
pub fn move_step(m: &mut Macro, from: usize, to: usize) {
    if from >= m.steps.len() || from == to {
        return;
    }
    let mut waits = step_waits(m);
    let step = m.steps.remove(from);
    let wait = waits.remove(from);
    let to = to.min(m.steps.len());
    m.steps.insert(to, step);
    waits.insert(to, wait);
    retime(m, &waits);
}

/// Removes step `index`; the steps after it keep their waits and so happen earlier.
pub fn remove_step(m: &mut Macro, index: usize) {
    if index >= m.steps.len() {
        return;
    }
    let mut waits = step_waits(m);
    m.steps.remove(index);
    waits.remove(index);
    retime(m, &waits);
}

/// Adds a step after the last one. After a button press it releases that button.
pub fn add_step(m: &mut Macro) {
    let at = m.steps.last().map_or(0, |s| s.at_ms.saturating_add(DEFAULT_WAIT_MS).min(MAX_STEP_MS));
    let step = match m.steps.last() {
        Some(last) if last.event == EVENT_PRESS => Step { at_ms: at, key: last.key.clone(), event: EVENT_RELEASE },
        _ => Step { at_ms: at, key: "a".to_owned(), event: EVENT_PRESS },
    };
    m.steps.push(step);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_names_are_translated_for_display() {
        assert_eq!(display_name("格斗游戏配置"), "Fighting");
        assert_eq!(display_name("Gyro"), "Gyro");
        assert_eq!(display_name("格斗"), "格斗");
    }

    fn blob() -> Vec<u8> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../core/testdata");
        let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        files.sort();
        let f = files.into_iter().find(|p| p.extension().is_some_and(|x| x == "bin")).unwrap();
        std::fs::read(f).unwrap()
    }

    fn base() -> (Config, Vec<u8>) {
        let b = blob();
        let mut c = Config::decode(&b).unwrap();
        c.macros.list.clear();
        c.buttons.0.clear();
        c.turbo.0.clear();
        (c, b)
    }

    fn step(at_ms: u32) -> Step {
        Step { at_ms, key: "a".to_owned(), event: EVENT_PRESS }
    }

    /// Encodes and decodes, so every rule is checked against what the controller would store.
    fn stored(c: &Config, b: &[u8]) -> Config {
        Config::decode(&c.apply(b).unwrap()).unwrap()
    }

    #[test]
    fn a_fresh_config_has_no_records_and_every_macro_is_unapplied() {
        let (c, _) = base();
        assert!((0..MAX_MACROS).all(|i| macro_button(&c, i).is_none()));
        assert_eq!(c.macros.list.len(), 0);
    }

    #[test]
    fn editing_a_later_macro_creates_parked_empty_records_before_it() {
        let (mut c, b) = base();
        edit_macro(&mut c, 2, |m| m.steps.push(step(0)));
        assert_eq!(c.macros.list.len(), 3);
        assert!(c.macros.list.iter().all(|m| is_parked_macro_button(&m.button)));
        assert!(c.macros.list.iter().take(2).all(|m| m.steps.is_empty()));
        assert_eq!(stored(&c, &b).macros, c.macros);
    }

    #[test]
    fn applying_moves_the_macro_and_frees_its_old_button() {
        let (mut c, b) = base();
        edit_macro(&mut c, 0, |m| m.steps.push(step(0)));
        apply_macro(&mut c, 0, "x");
        assert_eq!(macro_on_button(&c, "x"), Some(0));
        apply_macro(&mut c, 0, "y");
        assert_eq!(macro_on_button(&c, "x"), None);
        assert_eq!(macro_on_button(&c, "y"), Some(0));
        let s = stored(&c, &b);
        assert_eq!(s.buttons.get("y").map(String::as_str), Some("macro"));
        assert!(s.buttons.get("x").is_none());
    }

    #[test]
    fn applying_a_macro_to_a_taken_button_unapplies_the_other_one() {
        let (mut c, b) = base();
        edit_macro(&mut c, 0, |m| m.steps.push(step(0)));
        edit_macro(&mut c, 1, |m| m.steps.push(step(0)));
        apply_macro(&mut c, 0, "x");
        apply_macro(&mut c, 1, "x");
        assert_eq!(macro_on_button(&c, "x"), Some(1));
        assert_eq!(macro_button(&c, 0), None);
        assert_eq!(c.macros.list.first().map(|m| m.steps.len()), Some(1), "the steps stay");
        let s = stored(&c, &b);
        assert_eq!(s.macros, c.macros);
    }

    #[test]
    fn applying_replaces_the_buttons_mapping_and_turbo() {
        let (mut c, b) = base();
        set_mapping(&mut c, "x", Some("b"));
        set_turbo(&mut c, "x", Some(Turbo { rate: 10, mode: "hold".to_owned() }));
        edit_macro(&mut c, 0, |m| m.steps.push(step(0)));
        apply_macro(&mut c, 0, "x");
        let s = stored(&c, &b);
        assert_eq!(s.buttons.get("x").map(String::as_str), Some("macro"));
        assert!(turbo_of(&s, "x").is_none());
    }

    #[test]
    fn unapplying_keeps_the_steps_and_returns_the_button_to_its_default() {
        let (mut c, b) = base();
        edit_macro(&mut c, 0, |m| m.steps.push(step(0)));
        apply_macro(&mut c, 0, "x");
        unapply_macro(&mut c, 0);
        assert_eq!(macro_button(&c, 0), None);
        let s = stored(&c, &b);
        assert!(s.buttons.get("x").is_none());
        assert_eq!(s.macros.list.first().map(|m| m.steps.len()), Some(1));
    }

    #[test]
    fn clearing_removes_steps_and_trailing_records() {
        let (mut c, _) = base();
        edit_macro(&mut c, 1, |m| m.steps.push(step(0)));
        clear_macro(&mut c, 1);
        assert_eq!(c.macros.list.len(), 0);
    }

    #[test]
    fn clearing_keeps_the_macro_on_its_button() {
        let (mut c, b) = base();
        edit_macro(&mut c, 1, |m| m.steps.push(step(0)));
        apply_macro(&mut c, 1, "x");
        clear_macro(&mut c, 1);
        assert_eq!(macro_button(&c, 1), Some("x"));
        assert_eq!(c.macros.list.get(1).map(|m| m.steps.len()), Some(0));
        assert_eq!(stored(&c, &b).buttons.get("x").map(String::as_str), Some("macro"));
    }

    #[test]
    fn pauses_belong_to_their_slot() {
        let (mut c, b) = base();
        edit_macro(&mut c, 2, |m| m.steps.push(step(0)));
        c.macros.intervals_ms = vec![30, 30, 120, 30, 30];
        clear_macro(&mut c, 0);
        assert_eq!(c.macros.intervals_ms, vec![30, 30, 120, 30, 30]);
        assert_eq!(stored(&c, &b).macros.intervals_ms, c.macros.intervals_ms);
    }

    #[test]
    fn free_steps_count_the_records_a_new_macro_needs() {
        let (mut c, _) = base();
        // 538 - 6 header bytes = 532 = 133 units of 4 bytes; one record takes 1 unit.
        assert_eq!(macro_steps_free(&c, 0), 132);
        assert_eq!(macro_steps_free(&c, 4), 128);
        edit_macro(&mut c, 0, |m| m.steps.extend((0..10).map(|i| step(i * 10))));
        assert_eq!(macro_steps_free(&c, 0), 122);
    }

    fn timed(times: &[u32]) -> Macro {
        Macro { button: "a".to_owned(), mode: 1, steps: times.iter().map(|t| step(*t)).collect() }
    }

    fn times(m: &Macro) -> Vec<u32> {
        m.steps.iter().map(|s| s.at_ms).collect()
    }

    #[test]
    fn waits_count_from_the_previous_step() {
        let mut m = timed(&[0, 100, 150]);
        assert_eq!(step_waits(&m), vec![0, 100, 50]);
        set_step_wait(&mut m, 1, 200);
        assert_eq!(times(&m), vec![0, 200, 250]);
    }

    #[test]
    fn moving_a_step_keeps_its_wait() {
        let mut m = timed(&[10, 110, 130]);
        m.steps.iter_mut().zip(["x", "y", "z"]).for_each(|(s, k)| k.clone_into(&mut s.key));
        move_step(&mut m, 2, 0);
        assert_eq!(m.steps.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["z", "x", "y"]);
        assert_eq!(step_waits(&m), vec![20, 10, 100]);
    }

    #[test]
    fn removing_a_step_moves_the_later_ones_earlier() {
        let mut m = timed(&[0, 100, 150]);
        remove_step(&mut m, 1);
        assert_eq!(times(&m), vec![0, 50]);
    }

    #[test]
    fn a_step_after_a_press_releases_that_button() {
        let mut m = timed(&[0]);
        if let Some(s) = m.steps.first_mut() {
            "x".clone_into(&mut s.key);
        }
        add_step(&mut m);
        let last = m.steps.last().unwrap();
        assert_eq!((last.at_ms, last.key.as_str(), last.event), (100, "x", EVENT_RELEASE));
        add_step(&mut m);
        assert_eq!(m.steps.last().map(|s| s.event), Some(EVENT_PRESS));
    }

    #[test]
    fn the_first_step_starts_right_away() {
        let mut m = timed(&[]);
        add_step(&mut m);
        assert_eq!(times(&m), vec![0]);
    }
}
