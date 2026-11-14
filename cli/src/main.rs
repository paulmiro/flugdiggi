mod cli;
mod logging;

use std::path::Path;
use std::time::Duration;

use anstyle::{AnsiColor, Style};
use clap::Parser;
use color_eyre::eyre::{Report, WrapErr, bail};
use color_eyre::{Result, Section};
use flugdiggi_core::config::{self, Config};
use flugdiggi_core::led::{LED_LEN, Led};
use flugdiggi_core::{Blob, Controller, DeviceSummary, Error, Selector, Slot, Version, WriteOutcome};
use tracing::{info, instrument};

use crate::cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();
    logging::init(cli.verbose, cli.quiet)?;
    run(cli.command, &cli.device.unwrap_or_default())
}

fn run(command: Command, device: &Selector) -> Result<()> {
    match command {
        Command::List { mac } => list(mac),
        Command::Decode { file } => decode(&file),
        Command::Info => info(&mut open(device)?),
        Command::Slot { slot } => slot_command(&mut open(device)?, slot),
        Command::Export { slot, file } => {
            let text = toml::to_string(&open(device)?.read_config(slot)?)?;
            output(&text, file.as_deref(), &format!("config of slot {slot}"))
        }
        Command::LedExport { slot, file } => {
            let text = toml::to_string(&open(device)?.read_led(slot)?)?;
            output(&text, file.as_deref(), &format!("LED config of slot {slot}"))
        }
        Command::Import { slot, file } => {
            let wanted: Config = read_toml(&file)?;
            write_with_backup(&mut open(device)?, Blob::Mapping, slot, Version::New, |old| Ok(wanted.apply(old)?))
        }
        Command::LedImport { slot, file } => {
            let wanted: Led = read_toml(&file)?;
            write_with_backup(&mut open(device)?, Blob::Led, slot, Version::New, |old| Ok(wanted.apply(old)?))
        }
        Command::Rename { slot, name } => {
            let rename = |old: &[u8]| -> Result<Vec<u8>> {
                let mut cfg = Config::decode(old)?;
                cfg.name.clone_from(&name);
                Ok(cfg.apply(old)?)
            };
            write_with_backup(&mut open(device)?, Blob::Mapping, slot, Version::New, rename)
        }
        Command::Dump { slot, file } => dump(&mut open(device)?, Blob::Mapping, slot, &file),
        Command::LedDump { slot, file } => dump(&mut open(device)?, Blob::Led, slot, &file),
        Command::Restore { slot, file } => {
            let new = read_file(&file)?;
            Config::decode(&new).wrap_err_with(|| format!("{} is not a config blob", file.display()))?;
            let version = Version::Exact(config::data_version(&new)?);
            write_with_backup(&mut open(device)?, Blob::Mapping, slot, version, |_| Ok(new))
        }
        Command::LedRestore { slot, file } => {
            let new = read_file(&file)?;
            Led::decode(&new).wrap_err_with(|| format!("{} is not an LED blob", file.display()))?;
            write_with_backup(&mut open(device)?, Blob::Led, slot, Version::Keep, |_| Ok(new))
        }
        Command::Raw { commands } => raw(&mut open(device)?, &commands),
    }
}

#[instrument(level = "debug")]
fn open(device: &Selector) -> Result<Controller> {
    Controller::open(device).map_err(with_suggestion)
}

/// Adds hints on how to fix the problem for errors the user can act on.
fn with_suggestion(e: Error) -> Report {
    let hint = match &e {
        Error::NoDevice => Some(
            "connect the dongle or the USB cable; if it is connected, check that your user may access it (udev rule)",
        ),
        Error::SeveralDevices { .. } => Some("choose one with --device <PORT|MAC>, see `flugdiggi list`"),
        Error::NoDeviceOnPort(_) | Error::NoDeviceWithMac { .. } => {
            Some("see `flugdiggi list` (or `flugdiggi list --mac`)")
        }
        Error::Busy(_) => {
            Some("close other programs that use the controller's vendor interface (e.g. another flugdiggi)")
        }
        Error::Unresponsive(_) | Error::NoReply { .. } => Some("check that the controller is switched on and in range"),
        _ => None,
    };
    let report = Report::new(e);
    match hint {
        Some(h) => report.suggestion(h),
        None => report,
    }
}

fn list(probe: bool) -> Result<()> {
    let found = Controller::list(probe)?;
    if found.is_empty() {
        info!("no Flydigi controller connected");
    }
    let bold = Style::new().bold();
    for d in &found {
        let DeviceSummary { port, product, connection, connection_guessed, info } = d;
        let conn = if *connection_guessed { format!("{connection}?") } else { connection.to_string() };
        let color = if *connection_guessed {
            Style::new().dimmed()
        } else {
            Style::new().fg_color(Some(AnsiColor::Green.into()))
        };
        let port = format!("{port:<6}");
        match (probe, info) {
            (false, _) => anstream::println!("{bold}{port}{bold:#}  {color}{conn:<7}{color:#}  {product}"),
            (true, Some(i)) => anstream::println!(
                "{bold}{port}{bold:#}  {color}{conn:<7}{color:#}  {}  {product}  firmware {}",
                i.mac,
                i.firmware
            ),
            (true, None) => {
                anstream::println!("{bold}{port}{bold:#}  {color}{conn:<7}{color:#}  (no answer)  {product}");
            }
        }
    }
    Ok(())
}

fn info(c: &mut Controller) -> Result<()> {
    let i = c.info()?;
    let active = c.active_slot()?;
    let versions = c.slot_versions()?;
    let key = Style::new().bold();
    let row = |k: &str, v: String| anstream::println!("{key}{k:<14}{key:#} {v}");
    row("product", c.product().to_owned());
    row("port", c.port().to_string());
    row("connection", i.connection.to_string());
    row("MAC", i.mac.to_string());
    row("firmware", i.firmware.to_string());
    row("battery", i.battery.to_string());
    row("device type", format!("{:#04x}", i.device_type));
    row("chip type", i.chip_type.to_string());
    row("motion chip", i.motion_chip.to_string());
    row("active slot", active.slot.to_string());
    row("Switch slot", active.switch_mode_slot.map_or_else(|| "-".to_owned(), |s| s.to_string()));
    row("slot versions", versions.iter().map(|v| format!("{v:04x}")).collect::<Vec<_>>().join(" "));
    Ok(())
}

fn slot_command(c: &mut Controller, slot: Option<u8>) -> Result<()> {
    match slot {
        None => println!("{}", c.active_slot()?.slot),
        Some(n) => {
            let slot = Slot::new(n)?;
            c.set_active_slot(slot)?;
            info!("active slot is now {slot}");
        }
    }
    Ok(())
}

fn decode(file: &Path) -> Result<()> {
    let data = read_file(file)?;
    let text = if data.len() == LED_LEN {
        toml::to_string(&Led::decode(&data)?)?
    } else {
        toml::to_string(&Config::decode(&data)?)?
    };
    print!("{text}");
    Ok(())
}

#[instrument(level = "debug", skip(c))]
fn dump(c: &mut Controller, blob: Blob, slot: Slot, file: &Path) -> Result<()> {
    let data = c.read(blob, slot)?;
    std::fs::write(file, &data).wrap_err_with(|| format!("writing {}", file.display()))?;
    info!("{blob} of slot {slot} ({} bytes) written to {}", data.len(), file.display());
    Ok(())
}

/// Prints `text`, or writes it to `file`.
fn output(text: &str, file: Option<&Path>, what: &str) -> Result<()> {
    match file {
        Some(f) => {
            std::fs::write(f, text).wrap_err_with(|| format!("writing {}", f.display()))?;
            info!("{what} written to {}", f.display());
        }
        None => print!("{text}"),
    }
    Ok(())
}

fn read_file(file: &Path) -> Result<Vec<u8>> {
    std::fs::read(file).wrap_err_with(|| format!("reading {}", file.display()))
}

fn read_toml<T: serde::de::DeserializeOwned>(file: &Path) -> Result<T> {
    let text = std::fs::read_to_string(file).wrap_err_with(|| format!("reading {}", file.display()))?;
    toml::from_str(&text).wrap_err_with(|| format!("parsing {}", file.display()))
}

fn unix_time() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Writes the content `make` computes from the slot's current content, after backing that up.
#[instrument(level = "debug", skip(c, make))]
fn write_with_backup(
    c: &mut Controller,
    blob: Blob,
    slot: Slot,
    version: Version,
    make: impl FnOnce(&[u8]) -> Result<Vec<u8>>,
) -> Result<()> {
    let prepare = |old: &[u8]| -> Result<Vec<u8>> {
        let new = make(old)?;
        if old != new.as_slice() {
            let kind = if blob == Blob::Led { "led" } else { "config" };
            let backup = format!("flugdiggi-backup-{kind}-slot{slot}-{}.bin", unix_time());
            std::fs::write(&backup, old).wrap_err_with(|| format!("saving the backup {backup}"))?;
            info!("previous {blob} of slot {slot} saved to {backup}");
        }
        Ok(new)
    };
    let outcome = c.write(blob, slot, version, |old| prepare(old).map_err(Into::into));
    match outcome.wrap_err_with(|| format!("writing the {blob} of slot {slot}"))? {
        WriteOutcome::Written(_) => info!("slot {slot} written and verified"),
        WriteOutcome::Unchanged => info!("slot {slot} already has this {blob}, nothing written"),
    }
    Ok(())
}

fn raw(c: &mut Controller, commands: &[Vec<u8>]) -> Result<()> {
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ");
    for command in commands {
        if command.is_empty() {
            bail!("empty command");
        }
        let (sent, replies) = c.query_raw(command, Duration::from_millis(400))?;
        anstream::println!(">> {}", hex(&sent));
        for r in replies {
            anstream::println!("<< {:4}ms {}", r.after.as_millis(), hex(r.packet.get(14..).unwrap_or_default()));
        }
    }
    Ok(())
}
