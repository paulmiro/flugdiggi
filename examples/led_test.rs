//! Experiment: which LED write path persists? Uses slot 3 and restores it.
//! Usage: led_test <original-led3.bin>

use std::time::Duration;

use anyhow::{Result, ensure};
use flugdiggi::old::{Blob, Controller};

const SLOT: u8 = 3;

fn sleep(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn brightness(c: &Controller) -> Result<u8> {
    Ok(c.read_blob_raw(Blob::Led, SLOT)?[6])
}

fn main() -> Result<()> {
    let orig = std::fs::read(std::env::args().nth(1).expect("original LED file"))?;
    let mut c = Controller::open()?;
    c.t.trace = std::env::var("TRACE").is_ok();
    let active = c.current_config_id()?.0;
    ensure!(active != SLOT);
    ensure!(c.read_blob(Blob::Led, SLOT)? == orig, "LED slot {SLOT} differs from the original file");
    let mut modified = orig.clone();
    modified[6] = 20;

    if std::env::var("SKIP_A").is_err() {
        println!("A) partial write of packet 0");
        c.write_blob_partial(Blob::Led, SLOT, 0, &[&modified[..10]])?;
        sleep(500);
        println!("  active now {}, brightness read back (no switch): {}", c.current_config_id()?.0, brightness(&c)?);
        c.apply_config(active)?;
        sleep(1000);
        println!("  after switching away and reading again: {}", brightness(&c)?);
        c.apply_config(active)?;
    }

    println!("B) partial write + save (slot active), wait 3 s, switch away");
    c.write_blob_partial(Blob::Led, SLOT, 0, &[&modified[..10]])?;
    sleep(500);
    c.apply_config(SLOT)?;
    let ver = c.config_versions()?[SLOT as usize];
    c.save_active(ver)?;
    sleep(3000);
    c.apply_config(active)?;
    sleep(1000);
    println!("  after save + switch: brightness {}", brightness(&c)?);
    c.apply_config(active)?;

    println!("restore");
    let cur = c.read_blob(Blob::Led, SLOT)?;
    if cur != orig {
        c.write_blob_partial(Blob::Led, SLOT, 0, &[&orig[..10]])?;
        sleep(500);
        c.apply_config(SLOT)?;
        c.save_active(ver)?;
        sleep(3000);
        c.apply_config(active)?;
        sleep(1000);
    }
    let now = c.read_blob(Blob::Led, SLOT)?;
    println!("  LED slot {SLOT} {}; versions {:04x?}", if now == orig { "== original" } else { "DIFFERS" }, c.config_versions()?);
    Ok(())
}
