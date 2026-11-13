//! Mapping config blob (protocol version 3.x, 840 bytes) of old-protocol controllers.
//!
//! Offsets follow `MappingConfigParserV30`/`V31` of the official SDK. `Config::apply` only re-encodes
//! sections whose decoded value changed, so bytes we don't model are preserved exactly.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const CONFIG_LEN: usize = 840;

const KEYS: (usize, usize) = (13, 109);
const STICKS: usize = 109;
const TRIGGERS: usize = 123;
const MOTION: usize = 137;
const RUMBLE: usize = 145;
const TRIGGER_RUMBLE: usize = 154;
const TRIGGER_ADAPTER: usize = 185;
const DATA_VERSION: usize = 225;
const MACROS: (usize, usize) = (230, 768);
const NAME: (usize, usize) = (770, 786);
const STICKS_EXTRA: usize = 790;
const MACRO_INTERVALS: usize = 820;
const MOTION_SMOOTHNESS: usize = 830;

/// Physical/target key ids (`ControllerKey` in the SDK).
const KEY_NAMES: &[(u8, &str)] = &[
    (0, "up"), (1, "right"), (2, "down"), (3, "left"), (4, "a"), (5, "b"), (6, "select"), (7, "x"),
    (8, "y"), (9, "start"), (10, "lb"), (11, "rb"), (12, "lt"), (13, "rt"), (14, "ls"), (15, "rs"),
    (16, "c"), (17, "z"), (18, "m1"), (19, "m2"), (20, "m3"), (21, "m4"), (22, "m5"), (23, "m6"),
    (24, "menu"), (25, "turbo"), (27, "home"), (28, "back"), (160, "stick_center"), (161, "stick_up"),
    (162, "stick_up_right"), (163, "stick_right"), (164, "stick_down_right"), (165, "stick_down"),
    (166, "stick_down_left"), (167, "stick_left"), (168, "stick_up_left"), (240, "left_stick"),
    (241, "right_stick"), (242, "wheel"),
];

pub fn key_name(id: u8) -> String {
    KEY_NAMES.iter().find(|(i, _)| *i == id).map(|(_, n)| n.to_string()).unwrap_or(format!("key{id}"))
}

pub fn key_id(name: &str) -> Result<u8> {
    if let Some((i, _)) = KEY_NAMES.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)) {
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
    /// Per physical button (by name); only buttons present in the blob's 32-entry table.
    pub buttons: Vec<Button>,
    pub left_stick: Stick,
    pub right_stick: Stick,
    pub left_trigger: Curve,
    pub right_trigger: Curve,
    pub rumble: Rumble,
    pub trigger_rumble: TriggerRumble,
    pub motion: Motion,
    pub macros: Macros,
    /// Raw adaptive-trigger blocks (40 bytes, 2 × 20); semantics not yet modelled.
    pub trigger_adapter: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Button {
    pub button: String,
    /// "default", a key name, "macro" or "keyboard".
    pub map_to: String,
    /// Turbo: 0 = off, otherwise the firmware's frequency value.
    pub turbo: u8,
    /// Turbo activation: 0 = off, 1 = while pressed, 2 = toggled by click.
    pub turbo_mode: u8,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rumble {
    /// Raw main switch byte. The SDK treats 0 as "on", but the stock configs read 1 here.
    pub main_switch: u8,
    pub left: RumbleMotor,
    pub right: RumbleMotor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RumbleMotor {
    pub enabled: bool,
    pub min: u8,
    pub max: u8,
    pub scale: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerRumble {
    /// Raw main switch byte. The SDK writes 0 for "on" and 1 for "off"; stock configs read 1.
    pub main_switch: u8,
    pub left: TriggerMotor,
    pub right: TriggerMotor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerMotor {
    pub linear: MotorSetting,
    pub micro: MotorSetting,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotorSetting {
    pub kind: u8,
    pub min_level: u8,
    pub max_level: u8,
    pub filter: u8,
    pub min_start: u8,
    pub scale: u8,
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
    /// Repeat interval per macro in ms (firmware 3.1+, steps of 10 ms).
    pub intervals_ms: Vec<u16>,
    pub list: Vec<Macro>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    /// Button that triggers the macro.
    pub button: String,
    /// 0 = none, 1 = once, 2 = while pressed, 3 = toggle by click.
    pub mode: u8,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Time since macro start in ms (steps of 10 ms).
    pub at_ms: u32,
    pub key: String,
    /// 0 = release, 1 = press, 2 = left stick, 3 = right stick, 5 = hold.
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
        let buttons = (0..32)
            .map(|i| {
                let e = &d[KEYS.0 + 3 * i..KEYS.0 + 3 * i + 3];
                let map_to = match e[0] {
                    0xff => "default".to_string(),
                    32 => "macro".to_string(),
                    0xfe => "keyboard".to_string(),
                    k if k > 32 => "default".to_string(),
                    k => key_name(k),
                };
                Button { button: key_name(i as u8), map_to, turbo: e[2], turbo_mode: e[1] }
            })
            .collect();
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
        let motor = |o: usize| RumbleMotor { enabled: d[o] == 0, min: d[o + 1], max: d[o + 2], scale: d[o + 3] };
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
            left_stick: stick(0),
            right_stick: stick(1),
            left_trigger: Curve::decode(&d[TRIGGERS..]),
            right_trigger: Curve::decode(&d[TRIGGERS + 7..]),
            rumble: Rumble { main_switch: d[RUMBLE], left: motor(RUMBLE + 1), right: motor(RUMBLE + 5) },
            trigger_rumble: TriggerRumble { main_switch: d[TRIGGER_RUMBLE], left: tmotor(0), right: tmotor(1) },
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
        if self.buttons.len() != 32 {
            bail!("buttons must list all 32 entries");
        }
        for (i, (b, old)) in self.buttons.iter().zip(&cur.buttons).enumerate() {
            if b.button != old.button {
                bail!("button entries must keep their order (entry {i} is {:?})", old.button);
            }
            if b == old {
                continue;
            }
            let target = match b.map_to.as_str() {
                "default" => 0xff,
                "macro" => 32,
                "keyboard" => bail!("keyboard mapping is not supported"),
                k => key_id(k)?,
            };
            let o = KEYS.0 + 3 * i;
            d[o..o + 3].copy_from_slice(&[target, b.turbo_mode, b.turbo]);
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
            d[RUMBLE] = r.main_switch;
            for (o, m) in [(RUMBLE + 1, &r.left), (RUMBLE + 5, &r.right)] {
                d[o..o + 4].copy_from_slice(&[if m.enabled { 0 } else { 0xff }, m.min, m.max, m.scale]);
            }
        }
        if self.trigger_rumble != cur.trigger_rumble {
            let t = &self.trigger_rumble;
            d[TRIGGER_RUMBLE] = t.main_switch;
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
    fn remap_changes_only_that_entry() {
        let b = &backups()[0];
        let mut c = Config::decode(b).unwrap();
        c.buttons[18].map_to = "lb".into();
        let d = c.apply(b).unwrap();
        let diff: Vec<usize> = (0..d.len()).filter(|&i| d[i] != b[i]).collect();
        assert_eq!(diff, vec![KEYS.0 + 3 * 18]);
        assert_eq!(Config::decode(&d).unwrap().buttons[18].map_to, "lb");
    }
}
