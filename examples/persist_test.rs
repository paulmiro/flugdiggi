//! End-to-end check of the persistent write path (partial write + save) on slot 3:
//! change a mapping, switch away, read back, then restore the original byte for byte.
//! Usage: persist_test <original-slot3.bin>

use anyhow::{Result, ensure};
use flugdiggi::config::Config;
use flugdiggi::old::{Blob, Controller};

const SLOT: u8 = 3;

fn main() -> Result<()> {
    let orig = std::fs::read(std::env::args().nth(1).expect("original slot file"))?;
    let mut c = Controller::open()?;
    c.t.trace = std::env::var("TRACE").is_ok();
    let active = c.current_config_id()?.0;
    ensure!(active != SLOT, "slot {SLOT} must not be active at start");
    let versions = c.config_versions()?;
    println!("start: active {active}, versions {versions:04x?}");
    ensure!(c.read_blob(Blob::Mapping, SLOT)? == orig, "slot {SLOT} does not match the original file");

    let mut cfg = Config::decode(&orig)?;
    cfg.buttons.set("m5", "lb".into());
    let stored = c.write_config(SLOT, &cfg.apply(&orig)?, None)?;
    println!("written; active {}, versions {:04x?}", c.current_config_id()?.0, c.config_versions()?);
    let back = c.read_config(SLOT)?;
    println!("after switching away and back: {}", if back == stored { "edit KEPT" } else { "edit LOST" });
    println!("m5 maps to {}", Config::decode(&back)?.buttons.get("m5").map_or("itself", |s| s.as_str()));

    let version = u16::from_le_bytes([orig[225], orig[226]]);
    c.write_config(SLOT, &orig, Some(version))?;
    let now = c.read_config(SLOT)?;
    let v = c.config_versions()?;
    println!("restored: {}, versions {v:04x?}, active {}", if now == orig { "== original" } else { "DIFFERS" }, c.current_config_id()?.0);
    ensure!(now == orig && v == versions);
    Ok(())
}
