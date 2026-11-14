use std::time::Duration;

use anyhow::{Result, bail};
use flugdiggi::config::Config;
use flugdiggi::led::Led;
use flugdiggi::old::{Blob, Controller, Version};
use flugdiggi::transport::{hex, is_reply};

const USAGE: &str = "usage: flugdiggi [-v] [--device <port|MAC>] <command>

  list [--mac]                list connected Flydigi controllers by USB port and connection
                              (\"cable?\"/\"dongle?\" = guess); --mac also shows their MACs and the
                              exact connection but briefly disconnects them from games
  info                        device info, active slot, slot versions
  slot [N]                    show or switch the active slot (0-3)
  export <slot> [file.toml]   print or save a slot's config as TOML
  import <slot> <file.toml>   write a (modified) TOML config to a slot
  rename <slot> <name>        rename a slot's config
  dump <slot> <file.bin>      save a slot's raw config blob
  restore <slot> <file.bin>   write a raw config blob back to a slot
  decode <file.bin>           print a raw config blob as TOML (offline)
  led-export <slot> [file]    print or save a slot's LED config as TOML
  led-import <slot> <file>    write a (modified) TOML LED config to a slot
  led-dump <slot> <file.bin>  save a slot's raw LED blob
  led-restore <slot> <file>   write a raw LED blob back to a slot
  raw <cmd[:arg...]>...       send read-only commands and print all replies

--device is required when several controllers are connected. Selecting by MAC briefly
disconnects the other controllers from games; selecting by USB port (e.g. 3-3) does not.
Slots 4-7 are the Switch-mode configs. Every write first saves the slot's previous
content to flugdiggi-backup-<kind>-slot<N>-<time>.bin in the current directory.";

fn usage() -> ! {
    eprintln!("{USAGE}");
    std::process::exit(2)
}

fn parse_slot(s: &str) -> Result<u8> {
    let n: u8 = s.parse()?;
    if n > 7 {
        bail!("slot must be 0..=7");
    }
    Ok(n)
}

fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Writes the content `make` computes from the slot's current content, after backing that up.
fn write_with_backup(
    c: &Controller,
    blob: Blob,
    slot: u8,
    make: impl FnOnce(&[u8]) -> Result<Vec<u8>>,
    version: Version,
) -> Result<()> {
    let written = c.write_blob_with(
        blob,
        slot,
        |old| {
            let new = make(old)?;
            if old.len() != new.len() {
                bail!("new {blob:?} data has {} bytes, expected {}", new.len(), old.len());
            }
            if old != new {
                let kind = if matches!(blob, Blob::Led) { "led" } else { "config" };
                let backup = format!("flugdiggi-backup-{kind}-slot{slot}-{}.bin", unix_time());
                std::fs::write(&backup, old)?;
                println!("previous config of slot {slot} saved to {backup}");
            }
            Ok(new)
        },
        version,
    )?;
    match written {
        Some(_) => println!("slot {slot} written and verified"),
        None => println!("slot {slot} already has this config, nothing written"),
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut trace = false;
    let mut device = None;
    loop {
        match args.first().map(String::as_str) {
            Some("-v") => trace = true,
            Some("--device") if args.len() > 1 => device = Some(args.remove(1)),
            _ => break,
        }
        args.remove(0);
    }
    let Some(sub) = args.first().cloned() else { usage() };
    let arg = |i: usize| args.get(i).map(String::as_str);

    if sub == "decode" {
        let Some(file) = arg(1) else { usage() };
        let data = std::fs::read(file)?;
        let text = match data.len() {
            flugdiggi::led::LED_LEN => toml::to_string(&Led::decode(&data)?)?,
            _ => toml::to_string(&Config::decode(&data)?)?,
        };
        print!("{text}");
        return Ok(());
    }

    if sub == "list" {
        let probe = match arg(1) {
            None => false,
            Some("--mac") => true,
            _ => usage(),
        };
        for f in Controller::list(probe)? {
            let conn = match (f.wired, f.info.is_some()) {
                (true, true) => "cable  ",
                (false, true) => "dongle ",
                (true, false) => "cable? ",
                (false, false) => "dongle?",
            };
            match (probe, f.info) {
                (false, _) => println!("{}  {conn}  {}", f.port, f.product),
                (true, Some(i)) => println!("{}  {conn}  {}  {}  firmware {}", f.port, i.mac, f.product, i.firmware),
                (true, None) => println!("{}  {conn}  (no answer)  {}", f.port, f.product),
            }
        }
        return Ok(());
    }

    let mut c = Controller::open_device(device.as_deref())?;
    c.t.trace = trace;

    match (sub.as_str(), arg(1), arg(2)) {
        ("info", None, None) => {
            println!("product: {}", c.t.product);
            println!("{:#?}", c.heartbeat()?);
            let (cur, ns) = c.current_config_id()?;
            println!("active slot: {cur} (Switch-mode slot: {ns:?})");
            println!("slot versions: {:04x?}", c.config_versions()?);
        }
        ("slot", None, None) => println!("{}", c.current_config_id()?.0),
        ("slot", Some(n), None) => {
            let n: u8 = n.parse()?;
            if n > 3 {
                bail!("slot must be 0..=3");
            }
            c.apply_config(n)?;
            println!("active slot is now {n}");
        }
        ("export", Some(slot), file) => {
            let text = toml::to_string(&Config::decode(&c.read_config(parse_slot(slot)?)?)?)?;
            match file {
                Some(f) => std::fs::write(f, text)?,
                None => print!("{text}"),
            }
        }
        ("import", Some(slot), Some(file)) => {
            let slot = parse_slot(slot)?;
            let wanted: Config = toml::from_str(&std::fs::read_to_string(file)?)?;
            write_with_backup(&c, Blob::Mapping, slot, |old| wanted.apply(old), Version::New)?;
        }
        ("rename", Some(slot), Some(name)) => {
            let slot = parse_slot(slot)?;
            let rename = |old: &[u8]| {
                let mut cfg = Config::decode(old)?;
                cfg.name = name.to_string();
                cfg.apply(old)
            };
            write_with_backup(&c, Blob::Mapping, slot, rename, Version::New)?;
        }
        ("led-export", Some(slot), file) => {
            let text = toml::to_string(&Led::decode(&c.read_blob(Blob::Led, parse_slot(slot)?)?)?)?;
            match file {
                Some(f) => std::fs::write(f, text)?,
                None => print!("{text}"),
            }
        }
        ("led-import", Some(slot), Some(file)) => {
            let slot = parse_slot(slot)?;
            let wanted: Led = toml::from_str(&std::fs::read_to_string(file)?)?;
            write_with_backup(&c, Blob::Led, slot, |old| wanted.apply(old), Version::New)?;
        }
        ("led-dump", Some(slot), Some(file)) => {
            let data = c.read_blob(Blob::Led, parse_slot(slot)?)?;
            std::fs::write(file, &data)?;
            println!("{} bytes written to {file}", data.len());
        }
        ("led-restore", Some(slot), Some(file)) => {
            let slot = parse_slot(slot)?;
            let new = std::fs::read(file)?;
            Led::decode(&new)?;
            write_with_backup(&c, Blob::Led, slot, |_| Ok(new), Version::Keep)?;
        }
        ("dump", Some(slot), Some(file)) => {
            let data = c.read_config(parse_slot(slot)?)?;
            std::fs::write(file, &data)?;
            println!("{} bytes written to {file}", data.len());
        }
        ("restore", Some(slot), Some(file)) => {
            let slot = parse_slot(slot)?;
            let new = std::fs::read(file)?;
            Config::decode(&new)?;
            let version = Version::Exact(u16::from_le_bytes([new[225], new[226]]));
            write_with_backup(&c, Blob::Mapping, slot, |_| Ok(new), version)?;
        }
        ("raw", Some(_), _) => {
            for spec in &args[1..] {
                let bytes: Vec<u8> = spec.split(':').map(|b| u8::from_str_radix(b, 16)).collect::<Result<_, _>>()?;
                if !matches!(bytes.as_slice(), [0x10, ..] | [0x20] | [0x50, 0x04]) {
                    bail!("{spec} is not on the read-only allowlist");
                }
                let mut pkt = [0u8; 15];
                pkt[0] = 0xa5;
                pkt[1..1 + bytes.len()].copy_from_slice(&bytes);
                c.send_raw(&pkt)?;
                println!(">> {}", hex(&pkt));
                let t0 = std::time::Instant::now();
                while let Some(p) = c.t.read_until(Duration::from_millis(400).saturating_sub(t0.elapsed()), is_reply)? {
                    println!("<< {:4}ms {}", t0.elapsed().as_millis(), hex(&p[14..]));
                }
            }
        }
        _ => usage(),
    }
    Ok(())
}
