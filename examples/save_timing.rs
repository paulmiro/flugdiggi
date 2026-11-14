//! Save timing experiment on slot 3: changes the LED brightness (30 <-> 70) like `led-import`, saves,
//! then polls the controller (alternating 0x10 / 0x20) until `delay_ms` after the save and prints when it
//! answered. Then switches away and back, reads both blobs and reports whether the save persisted.
//! Usage: save_timing <delay_ms> [nopoll]

use std::time::{Duration, Instant};

use anyhow::Result;
use flugdiggi::old::{Blob, Controller};
use flugdiggi::transport::is_reply;

const SLOT: u8 = 3;
const BRIGHTNESS: usize = 6;
const NAME_PACKET: usize = 77;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let delay = Duration::from_millis(args[1].parse()?);
    let poll = args.get(2).is_none_or(|a| a != "nopoll");
    let c = Controller::open()?;
    let (orig, _) = c.current_config_id()?;

    c.apply_config(0)?;
    let mut edit = c.edit(SLOT)?;
    let led = c.read_blob(Blob::Led, SLOT)?;
    let map = c.read_blob(Blob::Mapping, SLOT)?;
    let mut new_led = led.clone();
    new_led[BRIGHTNESS] = if led[BRIGHTNESS] == 30 { 70 } else { 30 };
    let version = u16::from_le_bytes([map[225], map[226]]);
    edit.write(Blob::Led, 0, &[&new_led[..10]])?;
    edit.write(Blob::Mapping, NAME_PACKET, &[&map[10 * NAME_PACKET..10 * NAME_PACKET + 10]])?;

    edit.save(version)?;
    let saved = Instant::now();
    println!("brightness {} -> {}, saved", led[BRIGHTNESS], new_led[BRIGHTNESS]);

    // Timeline: one line per change between answering and silent.
    let mut answering: Option<bool> = None;
    let mut since = Duration::ZERO;
    let mut polls = 0;
    let mut gone_at = None;
    while saved.elapsed() < delay {
        if !poll {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        let id = if polls % 2 == 0 { 0x10 } else { 0x20 };
        polls += 1;
        let mut req = [0u8; 15];
        req[0] = 0xa5;
        req[1] = id;
        req[2] = polls as u8;
        let t = saved.elapsed();
        let ok = c.send_raw(&req).is_ok()
            && matches!(c.t.read_until(Duration::from_millis(100), |p| is_reply(p) && p[15] == id), Ok(Some(_)));
        if gone_at.is_none() && c.t.is_gone() {
            gone_at = Some(t);
            println!("{:6} ms  dongle re-enumerated", t.as_millis());
        }
        if answering != Some(ok) {
            if let Some(a) = answering {
                println!("{:6} ms  {} for {} ms", since.as_millis(), if a { "answering" } else { "SILENT" }, (t - since).as_millis());
            }
            answering = Some(ok);
            since = t;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if let Some(a) = answering {
        println!("{:6} ms  {} until the switch", since.as_millis(), if a { "answering" } else { "SILENT" });
    }

    let t = Instant::now();
    c.apply_config(0)?;
    println!("switch away took {} ms", t.elapsed().as_millis());
    std::thread::sleep(Duration::from_secs(1));
    c.apply_config(SLOT)?;
    std::thread::sleep(Duration::from_millis(500));
    let led_after = c.read_blob(Blob::Led, SLOT)?;
    let map_after = c.read_blob(Blob::Mapping, SLOT)?;
    let led_ok = led_after == new_led;
    let map_ok = map_after == map;
    println!(
        "result: delay {} ms, poll {poll}: LED {} (brightness {}), mapping {}",
        delay.as_millis(),
        if led_ok { "saved" } else { "NOT saved" },
        led_after[BRIGHTNESS],
        if map_ok { "unchanged" } else { "CHANGED" }
    );
    c.apply_config(orig)?;
    Ok(())
}
