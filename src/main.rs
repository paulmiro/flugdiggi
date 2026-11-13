use anyhow::{Result, bail};
use flugdiggi::old::Controller;
use flugdiggi::transport::hex;

fn usage() -> ! {
    eprintln!("usage: flugdiggi [-v] <info | slot [N] | dump-config <slot> <file>>");
    std::process::exit(2)
}

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let trace = args.first().is_some_and(|a| a == "-v");
    if trace {
        args.remove(0);
    }
    let Some(sub) = args.first() else { usage() };

    let mut c = Controller::open()?;
    c.t.trace = trace;

    match sub.as_str() {
        "info" => {
            println!("product: {}", c.t.product);
            println!("{:#?}", c.heartbeat()?);
            let (cur, ns) = c.current_config_id()?;
            println!("active slot: {cur} (switch mode slot: {ns:?})");
            println!("slot versions: {:04x?}", c.config_versions()?);
        }
        "slot" => match args.get(1) {
            None => println!("{}", c.current_config_id()?.0),
            Some(n) => {
                let n: u8 = n.parse()?;
                if n > 3 {
                    bail!("slot must be 0..=3");
                }
                c.apply_config(n)?;
                println!("active slot now {}", c.current_config_id()?.0);
            }
        },
        "dump-config" => {
            let (Some(slot), Some(file)) = (args.get(1), args.get(2)) else { usage() };
            let data = c.read_mapping_config_raw(slot.parse()?)?;
            std::fs::write(file, &data)?;
            println!("{} bytes written to {file}", data.len());
            if trace {
                for chunk in data.chunks(16) {
                    println!("{}", hex(chunk));
                }
            }
        }
        "set-name" => {
            let (Some(slot), Some(name)) = (args.get(1), args.get(2)) else { usage() };
            let slot: u8 = slot.parse()?;
            if slot > 7 {
                bail!("slot must be 0..=7");
            }
            if c.current_config_id()?.0 == slot {
                bail!("refusing to write the active slot");
            }
            let units: Vec<u16> = name.encode_utf16().collect();
            if units.len() > 8 {
                bail!("name too long (max 8 UTF-16 units)");
            }
            let old = c.read_mapping_config_raw(slot)?;
            let backup = format!("flugdiggi-backup-slot{slot}-{}.bin", unix_time());
            std::fs::write(&backup, &old)?;
            println!("backup written to {backup}");
            let mut new = old.clone();
            new[770..786].fill(0);
            for (i, u) in units.iter().enumerate() {
                new[770 + 2 * i..772 + 2 * i].copy_from_slice(&u.to_le_bytes());
            }
            let n = c.write_mapping_config_diff(slot, &old, &new)?;
            println!("{n} packets written");
            let back = c.read_mapping_config_raw(slot)?;
            if back == new {
                println!("verified: slot {slot} reads back as written");
            } else {
                let diff: Vec<usize> = (0..back.len()).filter(|&i| back[i] != new[i]).collect();
                bail!("read-back differs from what was written at offsets {diff:?}");
            }
        }
        "restore" => {
            let (Some(slot), Some(file)) = (args.get(1), args.get(2)) else { usage() };
            let slot: u8 = slot.parse()?;
            let new = std::fs::read(file)?;
            let old = c.read_mapping_config_raw(slot)?;
            let backup = format!("flugdiggi-backup-slot{slot}-{}.bin", unix_time());
            std::fs::write(&backup, &old)?;
            println!("backup written to {backup}");
            let n = c.write_mapping_config_diff(slot, &old, &new)?;
            println!("{n} packets written");
            if c.read_mapping_config_raw(slot)? != new {
                bail!("read-back differs from {file}");
            }
            println!("verified");
        }
        // Sends a sequence of read-only commands, e.g. `raw 20 20 10:5c 20`, and prints every reply seen.
        "raw" => {
            for spec in &args[1..] {
                let bytes: Vec<u8> = spec
                    .split(':')
                    .map(|b| u8::from_str_radix(b, 16))
                    .collect::<Result<_, _>>()?;
                let ok = matches!(bytes.as_slice(), [0x10, ..] | [0x20] | [0x21, _] | [0x50, 0x04]);
                if !ok {
                    bail!("{spec} is not on the read-only allowlist");
                }
                let mut pkt = [0u8; 15];
                pkt[0] = 0xa5;
                pkt[1..1 + bytes.len()].copy_from_slice(&bytes);
                c.send_raw(&pkt)?;
                println!(">> {}", hex(&pkt));
                let t0 = std::time::Instant::now();
                while let Some(p) = c.t.read_until(
                    std::time::Duration::from_millis(400).saturating_sub(t0.elapsed()),
                    flugdiggi::transport::is_reply,
                )? {
                    println!("<< {:4}ms {}", t0.elapsed().as_millis(), hex(&p[14..]));
                }
            }
        }
        _ => usage(),
    }
    Ok(())
}

fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
