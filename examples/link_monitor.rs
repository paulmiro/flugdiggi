//! Read-only link monitor: repeatedly reads an LED config, prints the read packet interval and any replies
//! that arrive unrequested in the following second.
//! Usage: link_monitor <seconds> [switch]
//!   without `switch`: reads the active slot's LED config
//!   with `switch`: each round switches to slot 3, reads its LED and mapping, switches back (like a write)

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use flugdiggi::old::{Blob, Controller};
use flugdiggi::transport::is_reply;

fn timed_read(c: &Controller, blob: Blob, slot: u8) -> String {
    let t = Instant::now();
    let packets = match blob {
        Blob::Mapping => 84,
        Blob::Led => 50,
    };
    match c.read_blob_raw(blob, slot) {
        Ok(_) => format!("{blob:?} {:4} ms ({:2} ms/pkt)", t.elapsed().as_millis(), t.elapsed().as_millis() / packets),
        Err(e) => format!("{blob:?} FAILED after {} ms: {e}", t.elapsed().as_millis()),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let total = Duration::from_secs(args[1].parse()?);
    let switch = args.get(2).is_some_and(|a| a == "switch");
    let c = Controller::open()?;
    let (home, _) = c.current_config_id()?;
    let start = Instant::now();
    while start.elapsed() < total {
        let mut line = format!("{:6.1} s ", start.elapsed().as_secs_f32());
        if switch {
            c.apply_config(3)?;
            line += &timed_read(&c, Blob::Led, 3);
            line += "  ";
            line += &timed_read(&c, Blob::Mapping, 3);
            c.apply_config(home)?;
        } else {
            line += &timed_read(&c, Blob::Led, home);
        }
        let mut unrequested: BTreeMap<u8, usize> = BTreeMap::new();
        let listen = Instant::now();
        while listen.elapsed() < Duration::from_secs(1) {
            if let Ok(Some(p)) = c.t.read_until(Duration::from_millis(100), is_reply) {
                *unrequested.entry(p[15]).or_default() += 1;
            }
        }
        if !unrequested.is_empty() {
            line += &format!("  unrequested: {unrequested:02x?}");
        }
        println!("{line}");
    }
    c.apply_config(home)?;
    Ok(())
}
