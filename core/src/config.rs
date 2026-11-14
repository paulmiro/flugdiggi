//! Mapping config blob (protocol version 3.x, 840 bytes) of old-protocol controllers.
//!
//! Offsets follow `MappingConfigParserV30`/`V31` of the official SDK. [`Config::apply`] only re-encodes
//! sections whose decoded value changed, so bytes we don't model are preserved exactly.
//! Unmodelled: 2 package length, 3-12 legacy LED block (superseded by the LED blob),
//! 183-184 dial ("Lunpan") mapping, 227-229 and the tail bytes reserved (0xff).

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::bytes::{arr, get, offset, put, set, sub, sub_mut, u16_le};
use crate::error::{Error, Result};
use crate::types::Blob;

pub const CONFIG_LEN: usize = 840;

/// Button table: 32 entries of (target, turbo mode, turbo rate).
pub(crate) const KEYS: Range<usize> = 13..109;
const BUTTONS: usize = 32;
const STICKS: usize = 109;
const TRIGGERS: usize = 123;
const MOTION: usize = 137;
const RUMBLE: usize = 145;
const TRIGGER_RUMBLE: usize = 154;
const TRIGGER_ADAPTER: Range<usize> = 185..225;
const DATA_VERSION: usize = 225;
const MACROS: Range<usize> = 230..768;
const NAME: Range<usize> = 770..786;
const STICKS_EXTRA: usize = 790;
const MACRO_INTERVALS: usize = 820;
const MOTION_SMOOTHNESS: usize = 830;

macro_rules! invalid {
    ($($t:tt)*) => { return Err(Error::InvalidConfig(format!($($t)*))) };
}

/// Physical/target key ids (`ControllerKey` in the SDK). On the Vader 4, `fn` is the FN button and
/// `home` the power button (Xbox logo / Switch home).
const KEY_NAMES: &[(u8, &str)] = &[
    (0, "up"),
    (1, "right"),
    (2, "down"),
    (3, "left"),
    (4, "a"),
    (5, "b"),
    (6, "select"),
    (7, "x"),
    (8, "y"),
    (9, "start"),
    (10, "lb"),
    (11, "rb"),
    (12, "lt"),
    (13, "rt"),
    (14, "ls"),
    (15, "rs"),
    (16, "c"),
    (17, "z"),
    (18, "m1"),
    (19, "m2"),
    (20, "m3"),
    (21, "m4"),
    (22, "m5"),
    (23, "m6"),
    (24, "fn"),
    (25, "turbo"),
    (27, "home"),
    (28, "back"),
    (160, "stick_center"),
    (161, "stick_up"),
    (162, "stick_up_right"),
    (163, "stick_right"),
    (164, "stick_down_right"),
    (165, "stick_down"),
    (166, "stick_down_left"),
    (167, "stick_left"),
    (168, "stick_up_left"),
    (240, "left_stick"),
    (241, "right_stick"),
    (242, "wheel"),
];

const KEY_ALIASES: &[(u8, &str)] =
    &[(10, "l1"), (11, "r1"), (12, "l2"), (13, "r2"), (14, "l3"), (15, "r3"), (24, "menu")];

/// Buttons of the Xbox 360 report that a button can be mapped to (0-15 plus home = guide, tested).
fn is_map_target(id: u8) -> bool {
    id <= 15 || id == 27
}

/// Mapping to an extra button (c, z, m1-m6) disables the button (tested with m1); the app does the same.
const EXTRA_BUTTONS: std::ops::RangeInclusive<u8> = 16..=23;
const TARGET_NONE: u8 = 18;

pub fn key_name(id: u8) -> String {
    KEY_NAMES.iter().find(|(i, _)| *i == id).map_or_else(|| format!("key{id}"), |(_, n)| (*n).to_owned())
}

pub fn key_id(name: &str) -> Result<u8> {
    if let Some((i, _)) = KEY_NAMES.iter().chain(KEY_ALIASES).find(|(_, n)| n.eq_ignore_ascii_case(name)) {
        return Ok(*i);
    }
    if let Some(n) = name.strip_prefix("key").and_then(|n| n.parse().ok()) {
        return Ok(n);
    }
    invalid!("unknown key name {name:?}")
}

fn button_name(i: usize) -> Result<String> {
    u8::try_from(i).map(key_name).map_err(|_| Error::Internal("button index out of range"))
}

/// The data version (random id the official app changes on every save) of a mapping config.
pub fn data_version(d: &[u8]) -> Result<u16> {
    u16_le(d, DATA_VERSION)
}

pub(crate) fn set_data_version(d: &mut [u8], v: u16) -> Result<()> {
    put(d, DATA_VERSION, &v.to_le_bytes())
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
        if let Some(e) = self.0.iter_mut().find(|(n, _)| n == name) {
            e.1 = value;
        } else {
            self.0.push((name.to_owned(), value));
            self.0.sort_by_key(|(n, _)| key_id(n).unwrap_or(u8::MAX));
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
            let canon = key_id(&name).map_or_else(|_| name.clone(), key_name);
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
    TURBO_MODES.iter().find(|(i, _)| *i == id).map_or_else(|| format!("mode{id}"), |(_, n)| (*n).to_owned())
}

fn turbo_mode_id(name: &str) -> Result<u8> {
    if let Some((i, _)) = TURBO_MODES.iter().find(|(_, n)| *n == name) {
        return Ok(*i);
    }
    match name.strip_prefix("mode").and_then(|n| n.parse().ok()) {
        Some(n) => Ok(n),
        None => invalid!("unknown turbo mode {name:?}"),
    }
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
    fn decode(d: &[u8], off: usize) -> Result<Self> {
        let [kind, start, p1x, p1y, p2x, p2y, end] = arr(d, off)?;
        Ok(Curve { kind, start, point1: [p1x, p1y], point2: [p2x, p2y], end })
    }

    fn encode(&self, d: &mut [u8], off: usize) -> Result<()> {
        let [p1x, p1y] = self.point1;
        let [p2x, p2y] = self.point2;
        put(d, off, &[self.kind, self.start, p1x, p1y, p2x, p2y, self.end])
    }
}

impl MotorSetting {
    fn decode(d: &[u8], off: usize) -> Result<Self> {
        let [kind, min_level, max_level, filter, min_start, scale, min_time] = arr(d, off)?;
        Ok(MotorSetting { kind, min_level, max_level, filter, min_start, scale, min_time })
    }

    fn encode(&self, d: &mut [u8], off: usize) -> Result<()> {
        put(
            d,
            off,
            &[self.kind, self.min_level, self.max_level, self.filter, self.min_start, self.scale, self.min_time],
        )
    }
}

const MAX_MACROS: usize = 5;
const MACRO_TICK_MS: u32 = 10;
/// Macro offsets count in 4-byte units from the end of the header (count + offsets).
const MACRO_BASE: usize = MAX_MACROS + 1;

fn decode_macros(d: &[u8]) -> Vec<Macro> {
    let count = d.first().map_or(0, |&c| usize::from(c));
    if count == 0 || count > MAX_MACROS {
        return Vec::new();
    }
    // Start of macro `i`, from the offset table.
    let start_of =
        |i: usize| -> Option<usize> { usize::from(*d.get(i.checked_add(1)?)?).checked_mul(4)?.checked_add(MACRO_BASE) };
    let mut out = Vec::new();
    for i in 0..count {
        let Some(start) = start_of(i) else { continue };
        let end = if i.checked_add(1) == Some(count) {
            d.len()
        } else {
            i.checked_add(1).and_then(start_of).map_or(d.len(), |e| e.min(d.len()))
        };
        let Some(m) = d.get(start..end).filter(|m| m.len() >= 4) else { continue };
        let &[button, steps_n, _, mode, ..] = m else { continue };
        let steps = m
            .get(4..)
            .unwrap_or_default()
            .as_chunks::<4>()
            .0
            .iter()
            .take(usize::from(steps_n))
            .map(|&[lo, hi, key, event]| Step {
                at_ms: u32::from(u16::from_le_bytes([lo, hi])).saturating_mul(MACRO_TICK_MS),
                key: key_name(key),
                event,
            })
            .collect();
        out.push(Macro { button: key_name(button), mode, steps });
    }
    out
}

fn encode_macros(list: &[Macro], d: &mut [u8]) -> Result<()> {
    let too_long = || Error::InvalidConfig(format!("macros too long (max {} bytes)", d.len()));
    if list.len() > MAX_MACROS {
        invalid!("at most {MAX_MACROS} macros are supported");
    }
    let mut bytes = vec![0u8; MACRO_BASE];
    set(&mut bytes, 0, u8::try_from(list.len()).map_err(|_| Error::Internal("macro count"))?)?;
    // Offset table: entry i+1 is the start of macro i (in 4-byte units); macro 0 starts at 0.
    let mut units = 0usize;
    for (j, m) in list.iter().enumerate().take(list.len().saturating_sub(1)) {
        units = m.steps.len().checked_add(1).and_then(|n| units.checked_add(n)).ok_or_else(too_long)?;
        let entry = j.checked_add(2).ok_or(Error::Internal("macro index"))?;
        set(&mut bytes, entry, u8::try_from(units).map_err(|_| too_long())?)?;
    }
    for m in list {
        let n = u16::try_from(m.steps.len()).map_err(|_| too_long())?;
        let [lo, hi] = n.to_le_bytes();
        bytes.extend_from_slice(&[key_id(&m.button)?, lo, hi, m.mode]);
        for s in &m.steps {
            let Ok(t) = u16::try_from(s.at_ms.checked_div(MACRO_TICK_MS).unwrap_or_default()) else {
                invalid!("macro step at {} ms is too late", s.at_ms);
            };
            let [lo, hi] = t.to_le_bytes();
            bytes.extend_from_slice(&[lo, hi, key_id(&s.key)?, s.event]);
        }
    }
    if bytes.len() > d.len() {
        invalid!("macros too long ({} bytes, max {})", bytes.len(), d.len());
    }
    d.fill(0xff);
    put(d, 0, &bytes)
}

impl Config {
    pub fn decode(d: &[u8]) -> Result<Config> {
        if d.len() != CONFIG_LEN || d.get(1) != Some(&3) {
            let version = [d.first().copied().unwrap_or(0), d.get(1).copied().unwrap_or(0)];
            return Err(Error::UnsupportedBlob { blob: Blob::Mapping, len: d.len(), version });
        }
        let (mut buttons, mut turbo) = (Named(Vec::new()), Named(Vec::new()));
        for (i, e) in sub(d, KEYS)?.as_chunks::<3>().0.iter().enumerate() {
            let &[target, mode, rate] = e;
            let name = button_name(i)?;
            if target != TARGET_DEFAULT {
                buttons.0.push((name.clone(), target_name(target)));
            }
            if rate != 0 {
                turbo.0.push((name, Turbo { rate, mode: turbo_mode_name(mode) }));
            }
        }
        let stick = |i: usize| -> Result<Stick> {
            let [curve_type_ext, p0, p1, p2, p3, p4, p5, p6, p7, p8, circularity, edge] =
                arr(d, offset(STICKS_EXTRA, 12, i)?)?;
            Ok(Stick {
                curve: Curve::decode(d, offset(STICKS, 7, i)?)?,
                curve_type_ext,
                curve_points: vec![p0, p1, p2, p3, p4, p5, p6, p7, p8],
                circularity,
                edge,
            })
        };
        let motor = |o: usize| -> Result<RumbleMotor> {
            let [kind, min, max, scale] = arr(d, o)?;
            Ok(RumbleMotor { kind, min, max, scale })
        };
        let tmotor = |i: usize| -> Result<TriggerMotor> {
            Ok(TriggerMotor {
                linear: MotorSetting::decode(d, offset(TRIGGER_RUMBLE + 1, 14, i)?)?,
                micro: MotorSetting::decode(d, offset(TRIGGER_RUMBLE + 8, 14, i)?)?,
            })
        };
        let [map_to, enable_key, enable_mode, deadzone, sensitivity_x, sensitivity_y, use_mode, enable_key2] =
            arr(d, MOTION)?;
        let name_units: Vec<u16> = sub(d, NAME)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .take_while(|&u| u != 0 && u != 0xffff)
            .collect();
        let intervals: [u8; MAX_MACROS] = arr(d, MACRO_INTERVALS)?;
        Ok(Config {
            proto_version: u16_le(d, 0)?,
            data_version: data_version(d)?,
            name: String::from_utf16_lossy(&name_units),
            buttons,
            turbo,
            left_stick: stick(0)?,
            right_stick: stick(1)?,
            left_trigger: Curve::decode(d, TRIGGERS)?,
            right_trigger: Curve::decode(d, TRIGGERS + 7)?,
            rumble: Rumble { enabled: get(d, RUMBLE)? == 0, left: motor(RUMBLE + 1)?, right: motor(RUMBLE + 5)? },
            trigger_rumble: TriggerRumble {
                enabled: get(d, TRIGGER_RUMBLE)? == 0,
                left: tmotor(0)?,
                right: tmotor(1)?,
            },
            motion: Motion {
                map_to,
                enable_key: key_name(enable_key),
                enable_mode,
                deadzone,
                sensitivity_x,
                sensitivity_y,
                use_mode,
                enable_key2: key_name(enable_key2),
                smoothness: sub(d, MOTION_SMOOTHNESS..MOTION_SMOOTHNESS + 6)?.to_vec(),
            },
            macros: Macros {
                intervals_ms: intervals.iter().map(|&b| u16::from(b).saturating_mul(10)).collect(),
                list: decode_macros(sub(d, MACROS)?),
            },
            trigger_adapter: sub(d, TRIGGER_ADAPTER)?.to_vec(),
        })
    }

    /// Returns `orig` with every section of `self` that differs from `orig`'s decoded value re-encoded.
    pub fn apply(&self, orig: &[u8]) -> Result<Vec<u8>> {
        let cur = Config::decode(orig)?;
        let mut d = orig.to_vec();
        if self.proto_version != cur.proto_version || self.data_version != cur.data_version {
            invalid!("proto_version and data_version are read-only");
        }
        if self.name != cur.name {
            let bytes: Vec<u8> = self.name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            if bytes.len() > NAME.len() {
                invalid!("name too long (max 8 UTF-16 units)");
            }
            sub_mut(&mut d, NAME)?.fill(0);
            put(&mut d, NAME.start, &bytes)?;
        }
        self.apply_buttons(&cur, &mut d)?;
        for (i, (s, old)) in
            [(&self.left_stick, &cur.left_stick), (&self.right_stick, &cur.right_stick)].into_iter().enumerate()
        {
            if s.curve != old.curve {
                s.curve.encode(&mut d, offset(STICKS, 7, i)?)?;
            }
            let &[p0, p1, p2, p3, p4, p5, p6, p7, p8] = s.curve_points.as_slice() else {
                invalid!("curve_points must have 9 entries");
            };
            let extra = [s.curve_type_ext, p0, p1, p2, p3, p4, p5, p6, p7, p8, s.circularity, s.edge];
            put(&mut d, offset(STICKS_EXTRA, 12, i)?, &extra)?;
        }
        if self.left_trigger != cur.left_trigger {
            self.left_trigger.encode(&mut d, TRIGGERS)?;
        }
        if self.right_trigger != cur.right_trigger {
            self.right_trigger.encode(&mut d, TRIGGERS + 7)?;
        }
        if self.rumble != cur.rumble {
            let r = &self.rumble;
            if r.enabled != cur.rumble.enabled {
                set(&mut d, RUMBLE, u8::from(!r.enabled))?;
            }
            for (o, m) in [(RUMBLE + 1, &r.left), (RUMBLE + 5, &r.right)] {
                put(&mut d, o, &[m.kind, m.min, m.max, m.scale])?;
            }
        }
        if self.trigger_rumble != cur.trigger_rumble {
            let t = &self.trigger_rumble;
            if t.enabled != cur.trigger_rumble.enabled {
                set(&mut d, TRIGGER_RUMBLE, u8::from(!t.enabled))?;
            }
            for (i, m) in [&t.left, &t.right].into_iter().enumerate() {
                m.linear.encode(&mut d, offset(TRIGGER_RUMBLE + 1, 14, i)?)?;
                m.micro.encode(&mut d, offset(TRIGGER_RUMBLE + 8, 14, i)?)?;
            }
        }
        if self.motion != cur.motion {
            let m = &self.motion;
            let bytes = [
                m.map_to,
                key_id(&m.enable_key)?,
                m.enable_mode,
                m.deadzone,
                m.sensitivity_x,
                m.sensitivity_y,
                m.use_mode,
                key_id(&m.enable_key2)?,
            ];
            put(&mut d, MOTION, &bytes)?;
            if m.smoothness.len() != 6 {
                invalid!("motion.smoothness must have 6 entries");
            }
            put(&mut d, MOTION_SMOOTHNESS, &m.smoothness)?;
        }
        if self.macros.intervals_ms != cur.macros.intervals_ms {
            if self.macros.intervals_ms.len() != MAX_MACROS {
                invalid!("macros.intervals_ms must have {MAX_MACROS} entries");
            }
            let units: Vec<u8> =
                self.macros.intervals_ms.iter().map(|ms| u8::try_from(ms / 10).unwrap_or(u8::MAX)).collect();
            put(&mut d, MACRO_INTERVALS, &units)?;
        }
        if self.macros.list != cur.macros.list {
            encode_macros(&self.macros.list, sub_mut(&mut d, MACROS)?)?;
        }
        if self.trigger_adapter != cur.trigger_adapter {
            if self.trigger_adapter.len() != TRIGGER_ADAPTER.len() {
                invalid!("trigger_adapter must have {} bytes", TRIGGER_ADAPTER.len());
            }
            put(&mut d, TRIGGER_ADAPTER.start, &self.trigger_adapter)?;
        }
        Ok(d)
    }

    fn apply_buttons(&self, cur: &Config, d: &mut [u8]) -> Result<()> {
        for (table, names) in [
            ("buttons", self.buttons.0.iter().map(|e| &e.0).collect::<Vec<_>>()),
            ("turbo", self.turbo.0.iter().map(|e| &e.0).collect()),
        ] {
            for n in names {
                if !matches!(key_id(n), Ok(i) if usize::from(i) < BUTTONS) {
                    invalid!("[{table}]: {n:?} is not a button of the mapping table");
                }
            }
        }
        // The firmware marks a button that has a macro with the "macro" target by itself, so do the same.
        let mut macro_buttons = Vec::new();
        for m in &self.macros.list {
            let id = key_id(&m.button)?;
            if usize::from(id) >= BUTTONS {
                invalid!("[macros] button {:?} is not a button of the mapping table", m.button);
            }
            macro_buttons.push(key_name(id));
        }
        let macro_target = "macro".to_owned();
        for (i, entry) in sub_mut(d, KEYS)?.as_chunks_mut::<3>().0.iter_mut().enumerate() {
            let [target_b, mode_b, rate_b] = entry;
            let id = u8::try_from(i).map_err(|_| Error::Internal("button index out of range"))?;
            let name = key_name(id);
            let has_macro = macro_buttons.contains(&name);
            let target = match self.buttons.get(&name) {
                None if has_macro => Some(&macro_target),
                Some(t) if has_macro && t != "macro" => invalid!("[buttons] {name} = {t:?}, but {name} has a macro"),
                Some(t) if !has_macro && t == "macro" => {
                    invalid!("[buttons] {name} = \"macro\", but there is no macro for {name}")
                }
                t => t,
            };
            let old_target = cur.buttons.get(&name);
            let (turbo, old_turbo) = (self.turbo.get(&name), cur.turbo.get(&name));
            if target == old_target && turbo == old_turbo {
                continue;
            }
            if target != old_target {
                *target_b = match target.map(String::as_str) {
                    None => TARGET_DEFAULT,
                    Some("macro") => TARGET_MACRO,
                    // Extra buttons send nothing by themselves.
                    Some("none") if EXTRA_BUTTONS.contains(&id) => TARGET_DEFAULT,
                    Some("none") => TARGET_NONE,
                    Some("keyboard") => invalid!("keyboard mapping is not supported"),
                    Some(k) => match key_id(k)? {
                        t if t == id => TARGET_DEFAULT,
                        t if is_map_target(t) => t,
                        _ => invalid!(
                            "[buttons] {name} = {k:?}: can only map to an Xbox 360 button \
                             (up right down left a b x y select start lb rb lt rt ls rs home), \"none\" or \"macro\""
                        ),
                    },
                };
            }
            match turbo {
                // Without turbo the mode byte is meaningless; keep whatever the slot had.
                None => *rate_b = 0,
                Some(t) => {
                    if t.rate == 0 {
                        invalid!("[turbo] {name}: rate must be at least 1 (remove the entry to disable turbo)");
                    }
                    *mode_b = turbo_mode_id(&t.mode)?;
                    *rate_b = t.rate;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
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
        assert!(!v.is_empty(), "no test data");
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
            encode_macros(&c.macros.list, &mut d[MACROS]).unwrap();
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
        assert_eq!(d[RUMBLE], u8::from(!c.rumble.enabled));
    }

    #[test]
    fn remap_changes_only_that_entry() {
        let b = &backups()[0];
        let mut c = Config::decode(b).unwrap();
        c.buttons.set("m1", "lb".into());
        let d = c.apply(b).unwrap();
        let diff: Vec<usize> = (0..d.len()).filter(|&i| d[i] != b[i]).collect();
        assert_eq!(diff, vec![KEYS.start + 3 * 18]);
        assert_eq!(Config::decode(&d).unwrap().buttons.get("m1").unwrap(), "lb");
    }

    #[test]
    fn omitted_or_self_mapped_buttons_act_as_themselves() {
        let mut b = backups()[0].clone();
        b[KEYS.start + 3 * 18] = 4; // m1 -> a
        b[KEYS.start + 3 * 16] = 14; // c -> ls
        let mut c = Config::decode(&b).unwrap();
        assert_eq!(c.buttons.get("m1").unwrap(), "a");
        c.buttons.0.retain(|(n, _)| n != "m1");
        c.buttons.set("c", "c".into());
        let d = c.apply(&b).unwrap();
        assert_eq!(d[KEYS.start + 3 * 18], TARGET_DEFAULT);
        assert_eq!(d[KEYS.start + 3 * 16], TARGET_DEFAULT);
        let diff: Vec<usize> = (0..d.len()).filter(|&i| d[i] != b[i]).collect();
        assert_eq!(diff, vec![KEYS.start + 3 * 16, KEYS.start + 3 * 18]);
    }

    fn import_buttons(b: &[u8], buttons: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut t: toml::Table = toml::from_str(&toml::to_string(&Config::decode(b)?)?)?;
        t.insert("buttons".into(), toml::Value::Table(toml::from_str(buttons)?));
        let c: Config = toml::from_str(&toml::to_string(&t)?)?;
        Ok(c.apply(b)?)
    }

    #[test]
    fn aliases_and_target_validation() {
        let b = &backups()[0];
        let d = import_buttons(b, "L1 = \"r2\"\nm1 = \"l3\"\nmenu = \"a\"").unwrap();
        assert_eq!(d[KEYS.start + 3 * 10], 13);
        assert_eq!(d[KEYS.start + 3 * 18], 14);
        assert_eq!(d[KEYS.start + 3 * 24], 4);
        let c = Config::decode(&d).unwrap();
        assert_eq!(c.buttons.get("lb").unwrap(), "rt");
        assert_eq!(c.buttons.get("fn").unwrap(), "a");
        let d = import_buttons(b, "m1 = \"home\"\na = \"none\"\nm2 = \"none\"").unwrap();
        assert_eq!(d[KEYS.start + 3 * 18], 27);
        assert_eq!(d[KEYS.start + 3 * 4], TARGET_NONE);
        assert_eq!(d[KEYS.start + 3 * 19], TARGET_DEFAULT);
        let c = Config::decode(&d).unwrap();
        assert_eq!(c.buttons.get("a").unwrap(), "none");
        assert_eq!(c.buttons.get("m1").unwrap(), "home");
        for bad in
            ["a = \"m3\"", "a = \"fn\"", "a = \"key40\"", "a = \"left_stick\"", "l1 = \"a\"\nlb = \"b\"", "foo = \"a\""]
        {
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
        assert_eq!(d[KEYS.start + 3 * 4], TARGET_MACRO);
        let mut back = Config::decode(&d).unwrap();
        assert_eq!(back.buttons.get("a").unwrap(), "macro");
        back.buttons.set("a", "x".into());
        assert!(back.apply(&d).is_err());
        back.buttons.set("a", "macro".into());
        back.macros.list.clear();
        assert!(back.apply(&d).is_err());
        back.buttons.0.retain(|(n, _)| n != "a");
        assert_eq!(back.apply(&d).unwrap()[KEYS.start + 3 * 4], TARGET_DEFAULT);
    }

    #[test]
    fn turbo_round_trip_and_validation() {
        let b = &backups()[0];
        let mut c = Config::decode(b).unwrap();
        c.turbo.set("a", Turbo { rate: 10, mode: "toggle".into() });
        let d = c.apply(b).unwrap();
        assert_eq!(&d[KEYS.start + 12..KEYS.start + 15], &[b[KEYS.start + 12], 2, 10]);
        let back = Config::decode(&d).unwrap();
        assert_eq!(back.turbo.get("a").unwrap().mode, "toggle");
        c.buttons.set("nope", "a".into());
        assert!(c.apply(b).is_err());
    }
}
