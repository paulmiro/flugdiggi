//! LED config blob (version 2.0, 500 bytes) of old-protocol controllers (`RgbConfigParserV20`).
//!
//! Layout: version (2), type, loop_start, loop_end, loop_time, light_scale, rgb_num, led_mode, 11 reserved,
//! then 16 LEDs x 10 animation steps x RGB. Semantics were tested on a Vader 4 Pro, see workdir/logs.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const LED_LEN: usize = 500;
const HEADER: usize = 20;
const LEDS: usize = 16;
const STEPS: usize = 10;
/// The Vader 4 Pro has 5 LEDs in a V shape; these are LEDs 0-4 in blob order.
const POSITIONS: usize = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Led {
    /// Read-only blob version (0x0200).
    pub version: u16,
    /// 0: the animation loops. 1: every button press plays it once from step 0 and it stays on the last step.
    pub feedback: u8,
    /// Last animation step (0-9). The animation runs over steps 0..=loop_end, fading between steps.
    pub loop_end: u8,
    /// Duration of one step, about 100 ms per unit (1-100 in the official app).
    pub step_time: u8,
    /// 1-100 in the official app; has only a weak visible effect.
    pub brightness: u8,
    /// Ignored by the firmware; the official app's preset (1 flowing, 2 breathing, 3 gradient, 4 feedback,
    /// 5 steady, 6 off).
    pub app_preset: u8,
    /// Ignored by the firmware (the animation always starts at step 0).
    pub loop_start: u8,
    /// Ignored by the firmware; number of LEDs (5).
    pub led_count: u8,
    /// Slots for LEDs 5-15, which the Vader 4 Pro does not have.
    pub unused_leds: Vec<Vec<String>>,
    /// The 10 animation steps of each LED as "#rrggbb".
    pub leds: Leds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Leds {
    pub bottom: Vec<String>,
    pub middle_right: Vec<String>,
    pub top_right: Vec<String>,
    pub top_left: Vec<String>,
    pub middle_left: Vec<String>,
}

impl Leds {
    fn as_array(&self) -> [&Vec<String>; POSITIONS] {
        [&self.bottom, &self.middle_right, &self.top_right, &self.top_left, &self.middle_left]
    }
}

fn color(rgb: &[u8]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

fn parse_color(s: &str) -> Result<[u8; 3]> {
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.len() != 6 {
        bail!("colour {s:?} must be #rrggbb");
    }
    let v = u32::from_str_radix(h, 16)?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl Led {
    pub fn decode(d: &[u8]) -> Result<Led> {
        if d.len() != LED_LEN || d[1] != 2 || d[0] != 0 {
            bail!("unsupported LED blob (length {}, version {:02x}{:02x})", d.len(), d.get(1).unwrap_or(&0), d.first().unwrap_or(&0));
        }
        let steps = |led: usize| -> Vec<String> {
            (0..STEPS).map(|s| {
                let o = HEADER + 3 * (led * STEPS + s);
                color(&d[o..o + 3])
            }).collect()
        };
        Ok(Led {
            version: u16::from_le_bytes([d[0], d[1]]),
            feedback: d[2],
            loop_start: d[3],
            loop_end: d[4],
            step_time: d[5],
            brightness: d[6],
            led_count: d[7],
            app_preset: d[8],
            unused_leds: (POSITIONS..LEDS).map(steps).collect(),
            leds: Leds {
                bottom: steps(0),
                middle_right: steps(1),
                top_right: steps(2),
                top_left: steps(3),
                middle_left: steps(4),
            },
        })
    }

    /// Returns `orig` with the fields of `self` encoded; reserved bytes are kept.
    pub fn apply(&self, orig: &[u8]) -> Result<Vec<u8>> {
        let cur = Led::decode(orig)?;
        if self.version != cur.version {
            bail!("version is read-only");
        }
        if self.unused_leds.len() != LEDS - POSITIONS {
            bail!("unused_leds must have {} entries", LEDS - POSITIONS);
        }
        let all: Vec<&Vec<String>> = self.leds.as_array().into_iter().chain(self.unused_leds.iter()).collect();
        if all.iter().any(|s| s.len() != STEPS) {
            bail!("every LED must have {STEPS} steps");
        }
        let mut d = orig.to_vec();
        d[2..9].copy_from_slice(&[
            self.feedback,
            self.loop_start,
            self.loop_end,
            self.step_time,
            self.brightness,
            self.led_count,
            self.app_preset,
        ]);
        for (l, steps) in all.iter().enumerate() {
            for (s, c) in steps.iter().enumerate() {
                let o = HEADER + 3 * (l * STEPS + s);
                d[o..o + 3].copy_from_slice(&parse_color(c)?);
            }
        }
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn led_round_trip() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/led");
        for e in std::fs::read_dir(dir).unwrap() {
            let b = std::fs::read(e.unwrap().path()).unwrap();
            let l = Led::decode(&b).unwrap();
            let l2: Led = toml::from_str(&toml::to_string(&l).unwrap()).unwrap();
            assert_eq!(l, l2);
            assert_eq!(l2.apply(&b).unwrap(), b);
        }
    }
}
