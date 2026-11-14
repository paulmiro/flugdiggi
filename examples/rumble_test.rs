//! Plays XInput rumble on each given slot to compare the rumble / trigger-vibration switches.
//! Usage: rumble_test <slot>... (the active slot is restored afterwards)

use std::time::Duration;

use anyhow::Result;
use flugdiggi::old::Controller;

fn rumble(c: &Controller, left: u8, right: u8) -> Result<()> {
    c.t.write(&[0x00, 0x08, 0x00, left, right, 0x00, 0x00, 0x00])
}

fn main() -> Result<()> {
    let slots: Vec<u8> = std::env::args().skip(1).map(|a| a.parse().unwrap()).collect();
    let c = Controller::open()?;
    let (orig, _) = c.current_config_id()?;
    for s in slots {
        c.apply_config(s)?;
        std::thread::sleep(Duration::from_secs(2));
        for (label, l, r) in [("left (big) motor", 255, 0), ("right (small) motor", 0, 255)] {
            println!("slot {s}: {label} for 2 s");
            rumble(&c, l, r)?;
            std::thread::sleep(Duration::from_secs(2));
            rumble(&c, 0, 0)?;
            std::thread::sleep(Duration::from_secs(2));
        }
    }
    c.apply_config(orig)?;
    Ok(())
}
