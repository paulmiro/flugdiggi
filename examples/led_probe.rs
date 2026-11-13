//! Experiment without saving: what does the mapping working copy contain after an LED partial write?
//! Usage: led_probe <orig-slot3-mapping.bin> <orig-slot3-led.bin>

use anyhow::Result;
use flugdiggi::old::{Blob, Controller};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let map_orig = std::fs::read(&args[1])?;
    let led_orig = std::fs::read(&args[2])?;
    let mut c = Controller::open()?;
    c.t.trace = std::env::var("TRACE").is_ok();
    c.apply_config(1)?;
    c.apply_config(3)?;
    let m = c.read_blob_raw(Blob::Mapping, 3)?;
    println!("after switch to 3: mapping == slot3 orig: {}", m == map_orig);
    let l = c.read_blob_raw(Blob::Led, 3)?;
    println!("after reading LED 3: led brightness {}", l[6]);
    let m = c.read_blob_raw(Blob::Mapping, 3)?;
    println!("mapping still slot 3: {}", m == map_orig);
    c.write_blob_partial(Blob::Led, 3, 0, &[&led_orig[..10]])?;
    println!("after LED partial write: active {}", c.current_config_id()?.0);
    let m = c.read_blob_raw(Blob::Mapping, 3)?;
    println!("mapping working copy == slot3 orig: {} (name bytes {:02x?})", m == map_orig, &m[770..778]);
    let l = c.read_blob_raw(Blob::Led, 3)?;
    println!("LED working copy brightness {}", l[6]);
    c.apply_config(0)?;
    Ok(())
}
