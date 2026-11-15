//! Blocking controller operations, run on background threads.
//!
//! Each operation opens the controller and releases it when done, so games keep working while the GUI
//! is open. A process-wide lock keeps operations from overlapping.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use color_eyre::Result;
use color_eyre::eyre::WrapErr;
use flugdiggi_core::config::Config;
use flugdiggi_core::led::Led;
use flugdiggi_core::{
    ActiveSlot, Blob, Controller, DeviceInfo, DeviceSummary, Selector, Slot, UsbPort, Version, WriteOutcome,
};
use tracing::{debug, info};

use crate::model::{SLOTS, SlotData};

static IO: Mutex<()> = Mutex::new(());

fn open(port: &UsbPort) -> Result<Controller> {
    Controller::open(&Selector::Port(port.clone())).wrap_err_with(|| format!("opening the controller on port {port}"))
}

/// Lists connected controllers without claiming them.
pub fn list() -> Result<Vec<DeviceSummary>> {
    let _io = IO.lock().unwrap_or_else(PoisonError::into_inner);
    Ok(Controller::list(false)?)
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub info: DeviceInfo,
    pub active: ActiveSlot,
    pub slots: Vec<SlotData>,
}

/// Reads the device info and both blobs of every slot. `progress` counts finished slots.
pub fn load(port: &UsbPort, progress: &AtomicUsize) -> Result<Loaded> {
    let _io = IO.lock().unwrap_or_else(PoisonError::into_inner);
    let mut c = open(port)?;
    let info = c.info().wrap_err("reading the device info")?;
    let active = c.active_slot().wrap_err("reading the active slot")?;
    let numbers = (0..SLOTS).map(Slot::new).collect::<Result<Vec<_>, _>>()?;
    let slots = c
        .read_slots(&numbers, |done| progress.store(done, Ordering::Relaxed))
        .wrap_err("reading the slots")?
        .into_iter()
        .map(|(config, led)| SlotData::new(config, led))
        .collect();
    info!(%port, "read all slots");
    Ok(Loaded { info, active, slots })
}

/// Where the previous content of a slot is saved before every write.
pub fn backup_dir() -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME").map(|h| {
            let mut p = PathBuf::from(h);
            p.push(".local/share");
            p
        })
    });
    let mut dir = data.unwrap_or_else(std::env::temp_dir);
    dir.push("flugdiggi/backups");
    dir
}

fn backup(dir: &Path, blob: Blob, slot: Slot, data: &[u8]) -> Result<()> {
    std::fs::create_dir_all(dir).wrap_err_with(|| format!("creating {}", dir.display()))?;
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let kind = if blob == Blob::Led { "led" } else { "config" };
    let path = dir.join(format!("flugdiggi-backup-{kind}-slot{slot}-{time}.bin"));
    std::fs::write(&path, data).wrap_err_with(|| format!("saving the backup {}", path.display()))?;
    info!("previous {blob} of slot {slot} saved to {}", path.display());
    Ok(())
}

/// Writes the slot's edits and saves them. Returns the config and LED config as stored.
pub fn save(port: &UsbPort, slot: Slot, data: &SlotData) -> Result<(Config, Led)> {
    let _io = IO.lock().unwrap_or_else(PoisonError::into_inner);
    let mut c = open(port)?;
    let dir = backup_dir();
    let mut led = data.saved_led.clone();
    if data.led != data.saved_led {
        // Keep the data version: the mapping write below sets a new one, and an LED-only save must not
        // change the version our copy of the mapping config has.
        let wanted = data.led.clone();
        let out = c.write(Blob::Led, slot, Version::Keep, |old| {
            backup(&dir, Blob::Led, slot, old).map_err(|e| format!("{e:#}"))?;
            Ok(wanted.apply(old)?)
        })?;
        led = match out {
            WriteOutcome::Written(d) => Led::decode(&d)?,
            WriteOutcome::Unchanged => data.led.clone(),
        };
        debug!("LED config of slot {slot} written");
    }
    let mut config = data.saved_config.clone();
    if data.config != data.saved_config {
        let wanted = data.config.clone();
        let out = c.write(Blob::Mapping, slot, Version::New, |old| {
            backup(&dir, Blob::Mapping, slot, old).map_err(|e| format!("{e:#}"))?;
            Ok(wanted.apply(old)?)
        })?;
        config = match out {
            WriteOutcome::Written(d) => Config::decode(&d)?,
            WriteOutcome::Unchanged => data.config.clone(),
        };
        debug!("config of slot {slot} written");
    }
    info!("slot {slot} saved");
    Ok((config, led))
}

pub fn activate(port: &UsbPort, slot: Slot) -> Result<ActiveSlot> {
    let _io = IO.lock().unwrap_or_else(PoisonError::into_inner);
    let mut c = open(port)?;
    c.set_active_slot(slot)?;
    info!("slot {slot} is now active");
    Ok(c.active_slot()?)
}

/// A slot as exported to TOML.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SlotFile {
    pub config: Option<Config>,
    pub led: Option<Led>,
}

pub fn export(path: &Path, data: &SlotData) -> Result<()> {
    let file = SlotFile { config: Some(data.config.clone()), led: Some(data.led.clone()) };
    let text = toml::to_string(&file)?;
    std::fs::write(path, text).wrap_err_with(|| format!("writing {}", path.display()))?;
    info!("slot exported to {}", path.display());
    Ok(())
}

/// Reads a slot file, or a config or LED file exported by the command line tool.
pub fn import(path: &Path) -> Result<SlotFile> {
    let text = std::fs::read_to_string(path).wrap_err_with(|| format!("reading {}", path.display()))?;
    if let Ok(f) = toml::from_str::<SlotFile>(&text)
        && (f.config.is_some() || f.led.is_some())
    {
        return Ok(f);
    }
    if let Ok(config) = toml::from_str::<Config>(&text) {
        return Ok(SlotFile { config: Some(config), led: None });
    }
    let led = toml::from_str::<Led>(&text)
        .wrap_err_with(|| format!("{} is not a slot, config or LED file", path.display()))?;
    Ok(SlotFile { config: None, led: Some(led) })
}
