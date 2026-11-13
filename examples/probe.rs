//! Read-only probe for the Flydigi old (XInput) protocol over the raw XUSB pipe.
//!
//! Usage: probe [--knock] [--cmd 0xNN]...   (default command list: heartbeat 0x10)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use rusb::{DeviceHandle, GlobalContext};

const VID: u16 = 0x045e;
const PID: u16 = 0x028e;
const IFACE: u8 = 0;
const EP_IN: u8 = 0x81;
const EP_OUT: u8 = 0x05;

// Commands that are known not to change controller state.
const SAFE_CMDS: &[u8] = &[0x10];

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

fn write(h: &DeviceHandle<GlobalContext>, data: &[u8]) -> Result<()> {
    let n = h.write_interrupt(EP_OUT, data, Duration::from_millis(500))?;
    println!("OUT ({n}): {}", hex(data));
    Ok(())
}

fn main() -> Result<()> {
    let mut knock = false;
    let mut cmds: Vec<u8> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--knock" => knock = true,
            "--cmd" => {
                let v = args.next().context("--cmd needs a value")?;
                let c = u8::from_str_radix(v.trim_start_matches("0x"), 16)?;
                if !SAFE_CMDS.contains(&c) {
                    bail!("command 0x{c:02x} is not on the safe list");
                }
                cmds.push(c);
            }
            _ => bail!("unknown argument {a}"),
        }
    }
    if cmds.is_empty() {
        cmds.push(0x10);
    }

    let h = rusb::open_device_with_vid_pid(VID, PID).context("device 045e:028e not found")?;
    h.set_auto_detach_kernel_driver(true)?;
    h.claim_interface(IFACE)?;
    let h = Arc::new(h);

    let stop = Arc::new(AtomicBool::new(false));
    let reader = {
        let h = h.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let mut buf = [0u8; 64];
            let mut last_input: Vec<u8> = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                match h.read_interrupt(EP_IN, &mut buf, Duration::from_millis(100)) {
                    Ok(n) => {
                        let p = &buf[..n];
                        let is_ack = n > 15 && (p[14] == 0xa5 || p[14] == 0x5a);
                        // Plain input reports repeat constantly; only print when they change.
                        if is_ack || p != last_input.as_slice() {
                            let tag = if is_ack { "ACK" } else { "IN " };
                            println!("{tag} {:6}ms ({n:2}): {}", t0.elapsed().as_millis(), hex(p));
                        }
                        if !is_ack {
                            last_input = p.to_vec();
                        }
                    }
                    Err(rusb::Error::Timeout) => {}
                    Err(e) => {
                        println!("read error: {e}");
                        break;
                    }
                }
            }
        })
    };

    std::thread::sleep(Duration::from_millis(300));

    if knock {
        for (l, r) in [(0xab, 0xba), (0xba, 0xab), (0xab, 0xba), (0, 0)] {
            write(&h, &[0x00, 0x08, 0x00, l, r, 0x00, 0x00, 0x00])?;
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    for c in cmds {
        let mut pkt = [0u8; 15];
        pkt[0] = 0xa5;
        pkt[1] = c;
        if c == 0x10 {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .subsec_nanos();
            pkt[2] = (nanos & 0xff) as u8;
        }
        write(&h, &pkt)?;
        std::thread::sleep(Duration::from_millis(500));
    }

    std::thread::sleep(Duration::from_millis(500));
    stop.store(true, Ordering::Relaxed);
    reader.join().ok();
    h.release_interface(IFACE)?;
    Ok(())
}
