//! Experiment: does a full "write all" (0x25) config write survive a slot switch?
//! Uses slot 3 and restores it at the end. Usage: persist_test <original-slot3.bin>

use std::time::Duration;

use anyhow::{Result, ensure};
use flugdiggi::config::Config;
use flugdiggi::old::Controller;

const SLOT: u8 = 3;

fn read_retry(c: &Controller, slot: u8) -> Result<Vec<u8>> {
    for _ in 0..5 {
        match c.read_mapping_config_raw(slot) {
            Ok(d) => return Ok(d),
            Err(e) => {
                println!("  read failed ({e}), waiting");
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
    c.read_mapping_config_raw(slot)
}

fn main() -> Result<()> {
    let orig = std::fs::read(std::env::args().nth(1).expect("original slot file"))?;
    let c = Controller::open()?;
    let active = c.current_config_id()?.0;
    ensure!(active != SLOT, "slot {SLOT} must not be active at start");
    let versions = c.config_versions()?;
    println!("start: active {active}, versions {versions:04x?}");
    ensure!(read_retry(&c, SLOT)? == orig, "slot {SLOT} does not match the original file");
    c.apply_config(active)?;

    let mut cfg = Config::decode(&orig)?;
    cfg.buttons[22].map_to = "lb".into();
    let modified = cfg.apply(&orig)?;

    println!("write-all modified config, switch away, read back");
    c.write_mapping_config_full(SLOT, &modified)?;
    std::thread::sleep(Duration::from_millis(500));
    println!("  active after write-all: {}", c.current_config_id()?.0);
    c.apply_config(active)?;
    std::thread::sleep(Duration::from_millis(500));
    let after = read_retry(&c, SLOT)?;
    println!("  after switch: {}", if after == modified { "edit KEPT" } else if after == orig { "edit LOST" } else { "other" });
    println!("  versions: {:04x?}", c.config_versions()?);

    println!("restore with write-all");
    c.write_mapping_config_full(SLOT, &orig)?;
    std::thread::sleep(Duration::from_millis(500));
    c.apply_config(active)?;
    std::thread::sleep(Duration::from_millis(500));
    let after = read_retry(&c, SLOT)?;
    c.apply_config(active)?;
    let v = c.config_versions()?;
    println!("  restored: content {}, versions {v:04x?}", if after == orig { "== original" } else { "DIFFERS" });
    Ok(())
}
