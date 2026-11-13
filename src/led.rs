//! LED config blob (version 2.0, 500 bytes) of old-protocol controllers (`RgbConfigParserV20`).

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const LED_LEN: usize = 500;
const HEADER: usize = 20;
const FRAMES: usize = 16;
const LEDS_PER_FRAME: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Led {
    /// Read-only blob version (0x0200).
    pub version: u16,
    /// Light up on button press (1) or not (0).
    pub click_feedback: u8,
    /// First and last frame of the animation loop.
    pub loop_start: u8,
    pub loop_end: u8,
    /// Frame duration (firmware units).
    pub loop_time: u8,
    pub brightness: u8,
    /// Number of LEDs used per frame.
    pub leds: u8,
    /// Effect mode (firmware value).
    pub mode: u8,
    /// 16 animation frames of 10 colours each, as "#rrggbb".
    pub frames: Vec<Vec<String>>,
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
        let frames = (0..FRAMES)
            .map(|f| {
                (0..LEDS_PER_FRAME)
                    .map(|l| {
                        let o = HEADER + 3 * (f * LEDS_PER_FRAME + l);
                        color(&d[o..o + 3])
                    })
                    .collect()
            })
            .collect();
        Ok(Led {
            version: u16::from_le_bytes([d[0], d[1]]),
            click_feedback: d[2],
            loop_start: d[3],
            loop_end: d[4],
            loop_time: d[5],
            brightness: d[6],
            leds: d[7],
            mode: d[8],
            frames,
        })
    }

    /// Returns `orig` with the fields of `self` encoded; reserved bytes are kept.
    pub fn apply(&self, orig: &[u8]) -> Result<Vec<u8>> {
        let cur = Led::decode(orig)?;
        if self.version != cur.version {
            bail!("version is read-only");
        }
        if self.frames.len() != FRAMES || self.frames.iter().any(|f| f.len() != LEDS_PER_FRAME) {
            bail!("frames must be {FRAMES} lists of {LEDS_PER_FRAME} colours");
        }
        let mut d = orig.to_vec();
        d[2..9].copy_from_slice(&[
            self.click_feedback,
            self.loop_start,
            self.loop_end,
            self.loop_time,
            self.brightness,
            self.leds,
            self.mode,
        ]);
        for (f, frame) in self.frames.iter().enumerate() {
            for (l, c) in frame.iter().enumerate() {
                let o = HEADER + 3 * (f * LEDS_PER_FRAME + l);
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
