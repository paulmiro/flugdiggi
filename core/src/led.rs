//! LED config blob (version 2.0, 500 bytes) of old-protocol controllers (`RgbConfigParserV20`).
//!
//! Layout: version (2), type, `loop_start`, `loop_end`, `loop_time`, `light_scale`, `rgb_num`, `led_mode`, 11 reserved,
//! then 16 LEDs x 10 animation steps x RGB. Semantics were tested on a Vader 4 Pro, see workdir/logs.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::bytes::{arr, put, sub, sub_mut, u16_le};
use crate::error::{Error, Result};
use crate::types::Blob;

pub const LED_LEN: usize = 500;
const COLORS: Range<usize> = 20..20 + LEDS * STEPS * 3;
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
    /// Last animation step (0-9). The animation runs over steps `0..=loop_end`, fading between steps.
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

fn color([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn parse_color(s: &str) -> Result<[u8; 3]> {
    let h = s.strip_prefix('#').unwrap_or(s);
    match u32::from_str_radix(h, 16) {
        Ok(v) if h.len() == 6 => {
            let [_, r, g, b] = v.to_be_bytes();
            Ok([r, g, b])
        }
        _ => Err(Error::InvalidConfig(format!("colour {s:?} must be #rrggbb"))),
    }
}

impl Led {
    pub fn decode(d: &[u8]) -> Result<Led> {
        let [lo, hi]: [u8; 2] = arr(d, 0).unwrap_or_default();
        if d.len() != LED_LEN || hi != 2 || lo != 0 {
            return Err(Error::UnsupportedBlob { blob: Blob::Led, len: d.len(), version: [lo, hi] });
        }
        let colors: Vec<String> = sub(d, COLORS)?.as_chunks::<3>().0.iter().copied().map(color).collect();
        let mut leds = colors.chunks(STEPS).map(<[String]>::to_vec);
        let mut next = || leds.next().ok_or(Error::Internal("LED count"));
        let positions = Leds {
            bottom: next()?,
            middle_right: next()?,
            top_right: next()?,
            top_left: next()?,
            middle_left: next()?,
        };
        let [feedback, loop_start, loop_end, step_time, brightness, led_count, app_preset] = arr(d, 2)?;
        Ok(Led {
            version: u16_le(d, 0)?,
            feedback,
            loop_start,
            loop_end,
            step_time,
            brightness,
            led_count,
            app_preset,
            unused_leds: leds.collect(),
            leds: positions,
        })
    }

    /// Returns `orig` with the fields of `self` encoded; reserved bytes are kept.
    pub fn apply(&self, orig: &[u8]) -> Result<Vec<u8>> {
        let cur = Led::decode(orig)?;
        if self.version != cur.version {
            return Err(Error::InvalidConfig("version is read-only".into()));
        }
        if self.unused_leds.len() != LEDS - POSITIONS {
            return Err(Error::InvalidConfig(format!("unused_leds must have {} entries", LEDS - POSITIONS)));
        }
        let all: Vec<&Vec<String>> = self.leds.as_array().into_iter().chain(self.unused_leds.iter()).collect();
        if all.iter().any(|s| s.len() != STEPS) {
            return Err(Error::InvalidConfig(format!("every LED must have {STEPS} steps")));
        }
        let mut d = orig.to_vec();
        put(
            &mut d,
            2,
            &[
                self.feedback,
                self.loop_start,
                self.loop_end,
                self.step_time,
                self.brightness,
                self.led_count,
                self.app_preset,
            ],
        )?;
        let region = sub_mut(&mut d, COLORS)?;
        for (dst, c) in region.as_chunks_mut::<3>().0.iter_mut().zip(all.into_iter().flatten()) {
            *dst = parse_color(c)?;
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
