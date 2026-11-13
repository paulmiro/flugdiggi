//! One-off: restore slot 3's LED blob (brightness) via LED partial write + no-op mapping packet + save
//! with a long settle time, then verify both blobs from flash.
//! Usage: led_restore_test <orig-slot3-mapping.bin> <orig-slot3-led.bin>

use std::time::Duration;

use anyhow::Result;
use flugdiggi::old::{Blob, Controller};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let map_orig = std::fs::read(&args[1])?;
    let led_orig = std::fs::read(&args[2])?;
    let c = Controller::open()?;
    c.apply_config(1)?;
    c.apply_config(3)?;
    let m = c.read_blob_raw(Blob::Mapping, 3)?;
    anyhow::ensure!(m == map_orig, "slot 3 mapping not original");
    c.write_blob_partial(Blob::Led, 3, 0, &[&led_orig[..10]])?;
    c.write_blob_partial(Blob::Mapping, 3, 77, &[&map_orig[770..780]])?;
    anyhow::ensure!(c.current_config_id()?.0 == 3);
    c.save_active(u16::from_le_bytes([map_orig[225], map_orig[226]]))?;
    std::thread::sleep(Duration::from_secs(8));
    c.apply_config(0)?;
    std::thread::sleep(Duration::from_secs(1));
    let m = c.read_blob_raw(Blob::Mapping, 3)?;
    let l = c.read_blob_raw(Blob::Led, 3)?;
    c.apply_config(0)?;
    println!("mapping == orig: {}, led == orig: {} (brightness {})", m == map_orig, l == led_orig, l[6]);
    Ok(())
}
