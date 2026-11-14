//! Mapping config blob (protocol version 3.x, 840 bytes) of old-protocol controllers.
//!
//! Offsets follow `MappingConfigParserV30`/`V31` of the official SDK. `Config::apply` only re-encodes
//! sections whose decoded value changed, so bytes we don't model are preserved exactly.
//! Unmodelled: 2 package length, 3-12 legacy LED block (superseded by the LED blob),
//! 183-184 dial ("Lunpan") mapping, 227-229 and the tail bytes reserved (0xff).

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const CONFIG_LEN: usize = 840;

pub const KEYS: (usize, usize) = (13, 109);
const BUTTONS: usize = 32;
const STICKS: usize = 109;
const TRIGGERS: usize = 123;
const MOTION: usize = 137;
const RUMBLE: usize = 145;
const TRIGGER_RUMBLE: usize = 154;
const TRIGGER_ADAPTER: usize = 185;
pub const DATA_VERSION: usize = 225;
const MACROS: (usize, usize) = (230, 768);
const NAME: (usize, usize) = (770, 786);
const STICKS_EXTRA: usize = 790;
const MACRO_INTERVALS: usize = 820;
const MOTION_SMOOTHNESS: usize = 830;

/// Physical/target key ids (`ControllerKey` in the SDK). On the Vader 4, `fn` is the FN button and
/// `home` the power button (Xbox logo / Switch home).
const KEY_NAMES: &[(u8, &str)] = &[
    (0, "up"), (1, "right"), (2, "down"), (3, "left"), (4, "a"), (5, "b"), (6, "select"), (7, "x"),
    (8, "y"), (9, "start"), (10, "lb"), (11, "rb"), (12, "lt"), (13, "rt"), (14, "ls"), (15, "rs"),
    (16, "c"), (17, "z"), (18, "m1"), (19, "m2"), (20, "m3"), (21, "m4"), (22, "m5"), (23, "m6"),
    (24, "fn"), (25, "turbo"), (27, "home"), (28, "back"), (160, "stick_center"), (161, "stick_up"),
    (162, "stick_up_right"), (163, "stick_right"), (164, "stick_down_right"), (165, "stick_down"),
    (166, "stick_down_left"), (167, "stick_left"), (168, "stick_up_left"), (240, "left_stick"),
    (241, "right_stick"), (242, "wheel"),
];

const KEY_ALIASES: &[(u8, &str)] = &[
    (10, "l1"), (11, "r1"), (12, "l2"), (13, "r2"), (14, "l3"), (15, "r3"), (24, "menu"),
];

/// Buttons of the Xbox 360 report that a button can be mapped to (0-15 plus home = guide, tested).
fn is_map_target(id: u8) -> bool {
    id <= 15 || id == 27
}

/// Mapping to an extra button (c, z, m1-m6) disables the button (tested with m1); the app does the same.
const EXTRA_BUTTONS: std::ops::RangeInclusive<u8> = 16..=23;
const TARGET_NONE: u8 = 18;

pub fn key_name(id: u8) -> String {
    KEY_NAMES.iter().find(|(i, _)| *i == id).map(|(_, n)| n.to_string()).unwrap_or(format!("key{id}"))
}

pub fn key_id(name: &str) -> Result<u8> {
    if let Some((i, _)) = KEY_NAMES.iter().chain(KEY_ALIASES).find(|(_, n)| n.eq_ignore_ascii_case(name)) {
        return Ok(*i);
    }
    if let Some(n) = name.strip_prefix("key").and_then(|n| n.parse().ok()) {
        return Ok(n);
    }
    bail!("unknown key name {name:?}")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Read-only: protocol version of the blob (e.g. 0x0301).
    pub proto_version: u16,
    /// Read-only: random id the app changes on every permanent save.
    pub data_version: u16,
    pub name: String,
    /// Button remaps (`m1 = "a"`): a key name, "macro" or "keyboard". Buttons left out act as
    /// themselves (extra buttons without an Xbox equivalent then send nothing).
    #[serde(default)]
    pub buttons: Named<String>,
    /// Turbo per button; buttons left out have no turbo.
    #[serde(default)]
    pub turbo: Named<Turbo>,
    pub left_stick: Stick,
    pub right_stick: Stick,
    pub left_trigger: Curve,
    pub right_trigger: Curve,
    pub rumble: Rumble,
    pub trigger_rumble: TriggerRumble,
    pub motion: Motion,
    pub macros: Macros,
    /// Raw adaptive-trigger blocks (2 × 20 bytes: mode, vibration binding, mixed-mode params).
    /// Only used by the Apex series; the Vader 4 ignores them.
    pub trigger_adapter: Vec<u8>,
}

/// A TOML table keyed by button name, kept in button-table order.
#[derive(Debug, Clone, PartialEq)]
pub struct Named<T>(pub Vec<(String, T)>);

impl<T> Default for Named<T> {
    fn default() -> Self {
        Named(Vec::new())
    }
}

impl<T> Named<T> {
    pub fn get(&self, name: &str) -> Option<&T> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn set(&mut self, name: &str, value: T) {
        match self.0.iter_mut().find(|(n, _)| n == name) {
            Some(e) => e.1 = value,
            None => {
                self.0.push((name.to_string(), value));
                self.0.sort_by_key(|(n, _)| key_id(n).unwrap_or(u8::MAX));
            }
        }
    }
}

impl<T: Serialize> Serialize for Named<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_map(self.0.iter().map(|(k, v)| (k, v)))
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Named<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let m = std::collections::BTreeMap::<String, T>::deserialize(d)?;
        let mut v: Vec<(String, T)> = Vec::new();
        for (name, val) in m {
            let canon = key_id(&name).map(key_name).unwrap_or(name.clone());
            if let Some((other, _)) = v.iter().find(|(n, _)| key_id(n).map(key_name).ok().as_ref() == Some(&canon)) {
                return Err(serde::de::Error::custom(format!("{name:?} and {other:?} are the same button")));
            }
            v.push((canon, val));
        }
        v.sort_by_key(|(n, _)| key_id(n).unwrap_or(u8::MAX));
        Ok(Named(v))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turbo {
    /// Turbo rate, 1-30 in the official app.
    pub rate: u8,
    /// "hold" (while pressed), "toggle" (on/off by click) or "off".
    pub mode: String,
}

const TURBO_MODES: &[(u8, &str)] = &[(0, "off"), (1, "hold"), (2, "toggle")];

fn turbo_mode_name(id: u8) -> String {
    TURBO_MODES.iter().find(|(i, _)| *i == id).map(|(_, n)| n.to_string()).unwrap_or(format!("mode{id}"))
}

fn turbo_mode_id(name: &str) -> Result<u8> {
    if let Some((i, _)) = TURBO_MODES.iter().find(|(_, n)| *n == name) {
        return Ok(*i);
    }
    name.strip_prefix("mode").and_then(|n| n.parse().ok()).ok_or_else(|| anyhow::anyhow!("unknown turbo mode {name:?}"))
}

const TARGET_DEFAULT: u8 = 0xff;
const TARGET_MACRO: u8 = 32;
const TARGET_KEYBOARD: u8 = 0xfe;

fn target_name(id: u8) -> String {
    match id {
        TARGET_MACRO => "macro".into(),
        TARGET_KEYBOARD => "keyboard".into(),
        k if EXTRA_BUTTONS.contains(&k) => "none".into(),
        k => key_name(k),
    }
}

/// A response curve as stored by the firmware (raw 0..=127-ish units).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    /// 0 = default, 1 = quick, 2 = slow, 3 = custom.
    pub kind: u8,
    pub start: u8,
    pub point1: [u8; 2],
    pub point2: [u8; 2],
    pub end: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stick {
    pub curve: Curve,
    /// Extended curve (9 points), firmware 3.1+.
    pub curve_type_ext: u8,
    pub curve_points: Vec<u8>,
    /// 0 = square, 1 = round (as reported by the SDK's `JoystickCircularityType`).
    pub circularity: u8,
    pub edge: u8,
}

/// Grip rumble ("Controller Vibration" in the official app).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rumble {
    /// When false the controller ignores all rumble from the host (verified on hardware).
    pub enabled: bool,
    /// Left (low-frequency) motor.
    pub left: RumbleMotor,
    /// Right (high-frequency) motor.
    pub right: RumbleMotor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RumbleMotor {
    /// Motor type; not exposed by the official app.
    pub kind: u8,
    /// Output range 0-255; only editable in the official app's debug mode.
    pub min: u8,
    pub max: u8,
    /// Vibration intensity in % (the app keeps both motors equal).
    pub scale: u8,
}

/// Trigger rumble ("Trigger Vibration" in the official app).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerRumble {
    /// The app's on/off switch. The Vader 4 Pro has no trigger motors, so this block has no effect there.
    pub enabled: bool,
    pub left: TriggerMotor,
    pub right: TriggerMotor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerMotor {
    pub linear: MotorSetting,
    pub micro: MotorSetting,
}

/// The app only edits `linear` (both triggers alike, except min/max); `micro` and the
/// remaining fields are not exposed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotorSetting {
    pub kind: u8,
    /// Amplitude range 0-255 (shown as 0-100 in the app).
    pub min_level: u8,
    pub max_level: u8,
    /// "Masked value": rumble below this is ignored.
    pub filter: u8,
    /// `Vibrlimit` in the SDK.
    pub min_start: u8,
    /// Vibration intensity in %.
    pub scale: u8,
    /// `TimeLimit` in the SDK.
    pub min_time: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    /// 0 = off, 1 = left stick, 2 = right stick, 3 = mouse.
    pub map_to: u8,
    pub enable_key: String,
    pub enable_key2: String,
    /// 0 = toggle by click, 1 = while pressed.
    pub enable_mode: u8,
    pub deadzone: u8,
    pub sensitivity_x: u8,
    pub sensitivity_y: u8,
    /// 0 = FPS, 1 = racing.
    pub use_mode: u8,
    /// Smoothness curve (firmware 3.1+): zero, p1x, p1y, p2x, p2y, end.
    pub smoothness: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Macros {
    /// Pause between repetitions per macro in ms (modes 2 and 3; firmware 3.1+, steps of 10 ms).
    pub intervals_ms: Vec<u16>,
    pub list: Vec<Macro>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    /// Button that triggers the macro.
    pub button: String,
    /// 0 = none, 1 = once per press, 2 = repeat while held, 3 = repeat, toggled by click.
    pub mode: u8,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Time since macro start in ms (steps of 10 ms).
    pub at_ms: u32,
    pub key: String,
    /// 0 = release, 1 = press, 2 = left stick, 3 = right stick (key = stick_* direction, held until the
    /// next stick step), 5 = hold.
    pub event: u8,
}

impl Curve {
    fn decode(d: &[u8]) -> Self {
        Curve { kind: d[0], start: d[1], point1: [d[2], d[3]], point2: [d[4], d[5]], end: d[6] }
    }
    fn encode(&self, d: &mut [u8]) {
        d[..7].copy_from_slice(&[
            self.kind, self.start, self.point1[0], self.point1[1], self.point2[0], self.point2[1], self.end,
        ]);
    }
}

impl MotorSetting {
    fn decode(d: &[u8]) -> Self {
        MotorSetting { kind: d[0], min_level: d[1], max_level: d[2], filter: d[3], min_start: d[4], scale: d[5], min_time: d[6] }
    }
    fn encode(&self, d: &mut [u8]) {
        d[..7].copy_from_slice(&[
            self.kind, self.min_level, self.max_level, self.filter, self.min_start, self.scale, self.min_time,
        ]);
    }
}

const MAX_MACROS: usize = 5;
const MACRO_TICK_MS: u32 = 10;

fn decode_macros(d: &[u8]) -> Vec<Macro> {
    let count = d[0] as usize;
    if count == 0 || count > MAX_MACROS {
        return Vec::new();
    }
    let base = MAX_MACROS + 1;
    let mut out = Vec::new();
    for i in 0..count {
        let start = base + d[1 + i] as usize * 4;
        let end = if i == count - 1 { d.len() } else { (base + d[2 + i] as usize * 4).min(d.len()) };
        if start + 4 > end {
            continue;
        }
        let m = &d[start..end];
        let mut steps_n = m[1] as usize;
        if m.len() < steps_n * 4 {
            steps_n = (m.len() - 4) / 4;
        }
        let mut steps = Vec::new();
        for j in 0..steps_n {
            let o = 4 + 4 * j;
            if o + 3 >= m.len() {
                break;
            }
            steps.push(Step {
                at_ms: u16::from_le_bytes([m[o], m[o + 1]]) as u32 * MACRO_TICK_MS,
                key: key_name(m[o + 2]),
                event: m[o + 3],
            });
        }
        out.push(Macro { button: key_name(m[0]), mode: m[3], steps });
    }
    out
}

fn encode_macros(list: &[Macro], d: &mut [u8]) -> Result<()> {
    if list.len() > MAX_MACROS {
        bail!("at most {MAX_MACROS} macros are supported");
    }
    let mut bytes = vec![0u8; MAX_MACROS + 1];
    bytes[0] = list.len() as u8;
    let mut offset = 0;
    for (j, m) in list.iter().enumerate() {
        if j != list.len() - 1 {
            offset += m.steps.len() + 1;
            bytes[j + 2] = offset as u8;
        }
    }
    for m in list {
        let n = m.steps.len() as u16;
        bytes.extend_from_slice(&[key_id(&m.button)?, n as u8, (n >> 8) as u8, m.mode]);
        for s in &m.steps {
            let t = (s.at_ms / MACRO_TICK_MS) as u16;
            bytes.extend_from_slice(&[t as u8, (t >> 8) as u8, key_id(&s.key)?, s.event]);
        }
    }
    if bytes.len() > d.len() {
        bail!("macros too long ({} bytes, max {})", bytes.len(), d.len());
    }
    d.fill(0xff);
    d[..bytes.len()].copy_from_slice(&bytes);
    Ok(())
}

impl Config {
    pub fn decode(d: &[u8]) -> Result<Config> {
        if d.len() != CONFIG_LEN || d[1] != 3 {
            bail!("unsupported config blob (length {}, version byte {:#04x})", d.len(), d.get(1).unwrap_or(&0));
        }
        let (mut buttons, mut turbo) = (Named(Vec::new()), Named(Vec::new()));
        for i in 0..BUTTONS {
            let e = &d[KEYS.0 + 3 * i..KEYS.0 + 3 * i + 3];
            let name = key_name(i as u8);
            if e[0] != TARGET_DEFAULT {
                buttons.0.push((name.clone(), target_name(e[0])));
            }
            if e[2] != 0 {
                turbo.0.push((name, Turbo { rate: e[2], mode: turbo_mode_name(e[1]) }));
            }
        }
        let stick = |i: usize| {
            let x = STICKS_EXTRA + 12 * i;
            Stick {
                curve: Curve::decode(&d[STICKS + 7 * i..]),
                curve_type_ext: d[x],
                curve_points: d[x + 1..x + 10].to_vec(),
                circularity: d[x + 10],
                edge: d[x + 11],
            }
        };
        let motor = |o: usize| RumbleMotor { kind: d[o], min: d[o + 1], max: d[o + 2], scale: d[o + 3] };
        let tmotor = |i: usize| TriggerMotor {
            linear: MotorSetting::decode(&d[TRIGGER_RUMBLE + 1 + 14 * i..]),
            micro: MotorSetting::decode(&d[TRIGGER_RUMBLE + 8 + 14 * i..]),
        };
        let m = &d[MOTION..MOTION + 8];
        let name_units: Vec<u16> = d[NAME.0..NAME.1]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0 && u != 0xffff)
            .collect();
        Ok(Config {
            proto_version: u16::from_le_bytes([d[0], d[1]]),
            data_version: u16::from_le_bytes([d[DATA_VERSION], d[DATA_VERSION + 1]]),
            name: String::from_utf16_lossy(&name_units),
            buttons,
            turbo,
            left_stick: stick(0),
            right_stick: stick(1),
            left_trigger: Curve::decode(&d[TRIGGERS..]),
            right_trigger: Curve::decode(&d[TRIGGERS + 7..]),
            rumble: Rumble { enabled: d[RUMBLE] == 0, left: motor(RUMBLE + 1), right: motor(RUMBLE + 5) },
            trigger_rumble: TriggerRumble { enabled: d[TRIGGER_RUMBLE] == 0, left: tmotor(0), right: tmotor(1) },
            motion: Motion {
                map_to: m[0],
                enable_key: key_name(m[1]),
                enable_mode: m[2],
                deadzone: m[3],
                sensitivity_x: m[4],
                sensitivity_y: m[5],
                use_mode: m[6],
                enable_key2: key_name(m[7]),
                smoothness: d[MOTION_SMOOTHNESS..MOTION_SMOOTHNESS + 6].to_vec(),
            },
            macros: Macros {
                intervals_ms: d[MACRO_INTERVALS..MACRO_INTERVALS + MAX_MACROS].iter().map(|&b| b as u16 * 10).collect(),
                list: decode_macros(&d[MACROS.0..MACROS.1]),
            },
            trigger_adapter: d[TRIGGER_ADAPTER..TRIGGER_ADAPTER + 40].to_vec(),
        })
    }

    /// Returns `orig` with every section of `self` that differs from `orig`'s decoded value re-encoded.
    pub fn apply(&self, orig: &[u8]) -> Result<Vec<u8>> {
        let cur = Config::decode(orig)?;
        let mut d = orig.to_vec();
        if self.proto_version != cur.proto_version || self.data_version != cur.data_version {
            bail!("proto_version and data_version are read-only");
        }
        if self.name != cur.name {
            let units: Vec<u16> = self.name.encode_utf16().collect();
            if units.len() > 8 {
                bail!("name too long (max 8 UTF-16 units)");
            }
            d[NAME.0..NAME.1].fill(0);
            for (i, u) in units.iter().enumerate() {
                d[NAME.0 + 2 * i..NAME.0 + 2 * i + 2].copy_from_slice(&u.to_le_bytes());
            }
        }
        for (table, names) in [("buttons", self.buttons.0.iter().map(|e| &e.0).collect::<Vec<_>>()),
                               ("turbo", self.turbo.0.iter().map(|e| &e.0).collect())] {
            for n in names {
                if !matches!(key_id(n), Ok(i) if (i as usize) < BUTTONS) {
                    bail!("[{table}]: {n:?} is not a button of the mapping table");
                }
            }
        }
        // The firmware marks a button that has a macro with the "macro" target by itself, so do the same.
        let mut macro_buttons = Vec::new();
        for m in &self.macros.list {
            let id = key_id(&m.button)?;
            if id as usize >= BUTTONS {
                bail!("[macros] button {:?} is not a button of the mapping table", m.button);
            }
            macro_buttons.push(key_name(id));
        }
        let macro_target = "macro".to_string();
        for i in 0..BUTTONS {
            let name = key_name(i as u8);
            let has_macro = macro_buttons.contains(&name);
            let target = match self.buttons.get(&name) {
                None if has_macro => Some(&macro_target),
                Some(t) if has_macro && t != "macro" => bail!("[buttons] {name} = {t:?}, but {name} has a macro"),
                Some(t) if !has_macro && t == "macro" => bail!("[buttons] {name} = \"macro\", but there is no macro for {name}"),
                t => t,
            };
            let old_target = cur.buttons.get(&name);
            let (turbo, old_turbo) = (self.turbo.get(&name), cur.turbo.get(&name));
            if target == old_target && turbo == old_turbo {
                continue;
            }
            let o = KEYS.0 + 3 * i;
            if target != old_target {
                d[o] = match target.map(String::as_str) {
                    None => TARGET_DEFAULT,
                    Some("macro") => TARGET_MACRO,
                    // Extra buttons send nothing by themselves.
                    Some("none") if EXTRA_BUTTONS.contains(&(i as u8)) => TARGET_DEFAULT,
                    Some("none") => TARGET_NONE,
                    Some("keyboard") => bail!("keyboard mapping is not supported"),
                    Some(k) => match key_id(k)? {
                        id if id as usize == i => TARGET_DEFAULT,
                        id if is_map_target(id) => id,
                        _ => bail!(
                            "[buttons] {name} = {k:?}: can only map to an Xbox 360 button \
                             (up right down left a b x y select start lb rb lt rt ls rs home), \"none\" or \"macro\""
                        ),
                    },
                };
            }
            match turbo {
                // Without turbo the mode byte is meaningless; keep whatever the slot had.
                None => d[o + 2] = 0,
                Some(t) => {
                    if t.rate == 0 {
                        bail!("[turbo] {name}: rate must be at least 1 (remove the entry to disable turbo)");
                    }
                    d[o + 1] = turbo_mode_id(&t.mode)?;
                    d[o + 2] = t.rate;
                }
            }
        }
        for (i, (s, old)) in [(&self.left_stick, &cur.left_stick), (&self.right_stick, &cur.right_stick)].into_iter().enumerate() {
            if s.curve != old.curve {
                s.curve.encode(&mut d[STICKS + 7 * i..]);
            }
            let x = STICKS_EXTRA + 12 * i;
            if s.curve_points.len() != 9 {
                bail!("curve_points must have 9 entries");
            }
            d[x] = s.curve_type_ext;
            d[x + 1..x + 10].copy_from_slice(&s.curve_points);
            d[x + 10] = s.circularity;
            d[x + 11] = s.edge;
        }
        if self.left_trigger != cur.left_trigger {
            self.left_trigger.encode(&mut d[TRIGGERS..]);
        }
        if self.right_trigger != cur.right_trigger {
            self.right_trigger.encode(&mut d[TRIGGERS + 7..]);
        }
        if self.rumble != cur.rumble {
            let r = &self.rumble;
            if r.enabled != cur.rumble.enabled {
                d[RUMBLE] = !r.enabled as u8;
            }
            for (o, m) in [(RUMBLE + 1, &r.left), (RUMBLE + 5, &r.right)] {
                d[o..o + 4].copy_from_slice(&[m.kind, m.min, m.max, m.scale]);
            }
        }
        if self.trigger_rumble != cur.trigger_rumble {
            let t = &self.trigger_rumble;
            if t.enabled != cur.trigger_rumble.enabled {
                d[TRIGGER_RUMBLE] = !t.enabled as u8;
            }
            for (i, m) in [&t.left, &t.right].into_iter().enumerate() {
                m.linear.encode(&mut d[TRIGGER_RUMBLE + 1 + 14 * i..]);
                m.micro.encode(&mut d[TRIGGER_RUMBLE + 8 + 14 * i..]);
            }
        }
        if self.motion != cur.motion {
            let m = &self.motion;
            d[MOTION..MOTION + 8].copy_from_slice(&[
                m.map_to,
                key_id(&m.enable_key)?,
                m.enable_mode,
                m.deadzone,
                m.sensitivity_x,
                m.sensitivity_y,
                m.use_mode,
                key_id(&m.enable_key2)?,
            ]);
            if m.smoothness.len() != 6 {
                bail!("motion.smoothness must have 6 entries");
            }
            d[MOTION_SMOOTHNESS..MOTION_SMOOTHNESS + 6].copy_from_slice(&m.smoothness);
        }
        if self.macros.intervals_ms != cur.macros.intervals_ms {
            if self.macros.intervals_ms.len() != MAX_MACROS {
                bail!("macros.intervals_ms must have {MAX_MACROS} entries");
            }
            for (i, ms) in self.macros.intervals_ms.iter().enumerate() {
                d[MACRO_INTERVALS + i] = (ms / 10).min(255) as u8;
            }
        }
        if self.macros.list != cur.macros.list {
            encode_macros(&self.macros.list, &mut d[MACROS.0..MACROS.1])?;
        }
        if self.trigger_adapter != cur.trigger_adapter {
            if self.trigger_adapter.len() != 40 {
                bail!("trigger_adapter must have 40 bytes");
            }
            d[TRIGGER_ADAPTER..TRIGGER_ADAPTER + 40].copy_from_slice(&self.trigger_adapter);
        }
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backups() -> Vec<Vec<u8>> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata");
        let mut v = Vec::new();
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_some_and(|x| x == "bin") {
                v.push(std::fs::read(p).unwrap());
            }
        }
        assert!(!v.is_empty());
        v
    }

    #[test]
    fn unchanged_config_round_trips_exactly() {
        for b in backups() {
            let c = Config::decode(&b).unwrap();
            let text = toml::to_string(&c).unwrap();
            let c2: Config = toml::from_str(&text).unwrap();
            assert_eq!(c, c2);
            assert_eq!(c2.apply(&b).unwrap(), b);
        }
    }

    #[test]
    fn reencoding_every_section_is_lossless() {
        for b in backups() {
            let c = Config::decode(&b).unwrap();
            let mut d = b.clone();
            encode_macros(&c.macros.list, &mut d[MACROS.0..MACROS.1]).unwrap();
            assert_eq!(Config::decode(&d).unwrap(), c, "macros re-encode changes meaning");
        }
    }

    #[test]
    fn rumble_switch_changes_only_its_byte() {
        let b = &backups()[0];
        let mut c = Config::decode(b).unwrap();
        c.rumble.enabled = !c.rumble.enabled;
        let d = c.apply(b).unwrap();
        let diff: Vec<usize> = (0..d.len()).filter(|&i| d[i] != b[i]).collect();
        assert_eq!(diff, vec![RUMBLE]);
        assert_eq!(d[RUMBLE], !c.rumble.enabled as u8);
    }

    #[test]
    fn remap_changes_only_that_entry() {
        let b = &backups()[0];
        let mut c = Config::decode(b).unwrap();
        c.buttons.set("m1", "lb".into());
        let d = c.apply(b).unwrap();
        let diff: Vec<usize> = (0..d.len()).filter(|&i| d[i] != b[i]).collect();
        assert_eq!(diff, vec![KEYS.0 + 3 * 18]);
        assert_eq!(Config::decode(&d).unwrap().buttons.get("m1").unwrap(), "lb");
    }

    #[test]
    fn omitted_or_self_mapped_buttons_act_as_themselves() {
        let mut b = backups()[0].clone();
        b[KEYS.0 + 3 * 18] = 4; // m1 -> a
        b[KEYS.0 + 3 * 16] = 14; // c -> ls
        let mut c = Config::decode(&b).unwrap();
        assert_eq!(c.buttons.get("m1").unwrap(), "a");
        c.buttons.0.retain(|(n, _)| n != "m1");
        c.buttons.set("c", "c".into());
        let d = c.apply(&b).unwrap();
        assert_eq!(d[KEYS.0 + 3 * 18], TARGET_DEFAULT);
        assert_eq!(d[KEYS.0 + 3 * 16], TARGET_DEFAULT);
        let diff: Vec<usize> = (0..d.len()).filter(|&i| d[i] != b[i]).collect();
        assert_eq!(diff, vec![KEYS.0 + 3 * 16, KEYS.0 + 3 * 18]);
    }

    fn import_buttons(b: &[u8], buttons: &str) -> Result<Vec<u8>> {
        let mut t: toml::Table = toml::from_str(&toml::to_string(&Config::decode(b)?)?)?;
        t.insert("buttons".into(), toml::Value::Table(toml::from_str(buttons)?));
        let c: Config = toml::from_str(&toml::to_string(&t)?)?;
        c.apply(b)
    }

    #[test]
    fn aliases_and_target_validation() {
        let b = &backups()[0];
        let d = import_buttons(b, "L1 = \"r2\"\nm1 = \"l3\"\nmenu = \"a\"").unwrap();
        assert_eq!(d[KEYS.0 + 3 * 10], 13);
        assert_eq!(d[KEYS.0 + 3 * 18], 14);
        assert_eq!(d[KEYS.0 + 3 * 24], 4);
        let c = Config::decode(&d).unwrap();
        assert_eq!(c.buttons.get("lb").unwrap(), "rt");
        assert_eq!(c.buttons.get("fn").unwrap(), "a");
        let d = import_buttons(b, "m1 = \"home\"\na = \"none\"\nm2 = \"none\"").unwrap();
        assert_eq!(d[KEYS.0 + 3 * 18], 27);
        assert_eq!(d[KEYS.0 + 3 * 4], TARGET_NONE);
        assert_eq!(d[KEYS.0 + 3 * 19], TARGET_DEFAULT);
        let c = Config::decode(&d).unwrap();
        assert_eq!(c.buttons.get("a").unwrap(), "none");
        assert_eq!(c.buttons.get("m1").unwrap(), "home");
        for bad in ["a = \"m3\"", "a = \"fn\"", "a = \"key40\"", "a = \"left_stick\"", "l1 = \"a\"\nlb = \"b\"", "foo = \"a\""] {
            assert!(import_buttons(b, bad).is_err(), "{bad} was accepted");
        }
    }

    #[test]
    fn macro_button_gets_macro_target() {
        let b = &backups()[0];
        let mut c = Config::decode(b).unwrap();
        c.macros.list.push(Macro {
            button: "a".into(),
            mode: 1,
            steps: vec![Step { at_ms: 0, key: "b".into(), event: 1 }, Step { at_ms: 100, key: "b".into(), event: 0 }],
        });
        let d = c.apply(b).unwrap();
        assert_eq!(d[KEYS.0 + 3 * 4], TARGET_MACRO);
        let mut back = Config::decode(&d).unwrap();
        assert_eq!(back.buttons.get("a").unwrap(), "macro");
        back.buttons.set("a", "x".into());
        assert!(back.apply(&d).is_err());
        back.buttons.set("a", "macro".into());
        back.macros.list.clear();
        assert!(back.apply(&d).is_err());
        back.buttons.0.retain(|(n, _)| n != "a");
        assert_eq!(back.apply(&d).unwrap()[KEYS.0 + 3 * 4], TARGET_DEFAULT);
    }

    #[test]
    fn turbo_round_trip_and_validation() {
        let b = &backups()[0];
        let mut c = Config::decode(b).unwrap();
        c.turbo.set("a", Turbo { rate: 10, mode: "toggle".into() });
        let d = c.apply(b).unwrap();
        assert_eq!(&d[KEYS.0 + 12..KEYS.0 + 15], &[b[KEYS.0 + 12], 2, 10]);
        let back = Config::decode(&d).unwrap();
        assert_eq!(back.turbo.get("a").unwrap().mode, "toggle");
        c.buttons.set("nope", "a".into());
        assert!(c.apply(b).is_err());
    }
}
