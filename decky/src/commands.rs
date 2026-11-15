//! The helper's commands. Every controller operation opens the controller and releases it afterwards.

use std::error::Error as _;
use std::io::Read;
use std::path::Path;

use flugdiggi_core::config::{Config, Named};
use flugdiggi_core::{Blob, Controller, Error, Selector, Slot, UsbPort, Version, WriteOutcome};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::info;

use crate::cli::Command;
use crate::tables::{self, BUTTONS, TARGETS};

/// Highest slot of the plugin: 0-3 are the `XInput`-mode slots, 4-7 (Switch mode) are not exposed.
const MAX_SLOT: u8 = 3;

#[derive(Debug)]
pub struct Failure {
    pub kind: &'static str,
    pub message: String,
}

impl Failure {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Failure { kind, message: message.into() }
    }
}

impl From<Error> for Failure {
    fn from(e: Error) -> Self {
        let kind = match &e {
            Error::NoDevice | Error::NoDeviceOnPort(_) => "NoDevice",
            Error::Busy(_) => "Busy",
            Error::Disconnected { .. } => "Disconnected",
            Error::InvalidConfig(_) => "InvalidConfig",
            Error::Prepare(..) => "Prepare",
            _ => "Error",
        };
        // The Display of a core error names only the action; the cause is in the source chain.
        let mut message = e.to_string();
        let mut source = e.source();
        while let Some(s) = source {
            message.push_str(": ");
            message.push_str(&s.to_string());
            source = s.source();
        }
        Failure { kind, message }
    }
}

type Reply = Result<Value, Failure>;

pub fn run(command: Command) -> Reply {
    match command {
        Command::Meta => Ok(meta()),
        Command::List => list(),
        Command::Status { port } => status(&port),
        Command::ReadButtons { port, slot } => read_buttons(&port, check_slot(slot)?),
        Command::WriteButtons { port, slot, backup_dir } => write_buttons(&port, check_slot(slot)?, &backup_dir),
        Command::Activate { port, slot } => activate(&port, check_slot(slot)?),
    }
}

fn check_slot(slot: Slot) -> Result<Slot, Failure> {
    if slot.get() <= MAX_SLOT {
        Ok(slot)
    } else {
        Err(Failure::new("InvalidSlot", format!("slot {slot} is not available here (valid: 0-{MAX_SLOT})")))
    }
}

fn open(port: &UsbPort) -> Result<Controller, Failure> {
    Ok(Controller::open(&Selector::Port(port.clone()))?)
}

fn meta() -> Value {
    json!({
        "buttons": BUTTONS.iter().map(|b| json!({ "id": b, "label": tables::label(b), "extra": tables::is_extra(b) })).collect::<Vec<_>>(),
        "targets": TARGETS.iter().map(|t| json!({ "id": t, "label": tables::label(t) })).collect::<Vec<_>>(),
        "slots": MAX_SLOT.checked_add(1),
    })
}

fn list() -> Reply {
    let devices = Controller::list(false)?
        .into_iter()
        .map(|d| {
            json!({
                "port": d.port.as_str(),
                "product": d.product,
                "connection": d.connection.to_string(),
                "connection_guessed": d.connection_guessed,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "devices": devices }))
}

fn status(port: &UsbPort) -> Reply {
    let mut c = open(port)?;
    let info = c.info()?;
    let active = c.active_slot()?;
    Ok(json!({
        "info": {
            "firmware": info.firmware.to_string(),
            "battery": info.battery.to_string(),
            "connection": info.connection.to_string(),
            "mac": info.mac.to_string(),
        },
        "active_slot": active.slot.get(),
    }))
}

fn buttons_json(buttons: &Named<String>) -> Value {
    Value::Object(buttons.0.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect())
}

fn read_buttons(port: &UsbPort, slot: Slot) -> Reply {
    let config = open(port)?.read_config(slot)?;
    Ok(json!({
        "slot": slot.get(),
        "name": config.name,
        "buttons": buttons_json(&config.buttons),
        "data_version": config.data_version,
    }))
}

#[derive(Debug, Deserialize)]
struct WriteRequest {
    buttons: std::collections::BTreeMap<String, String>,
    expected_data_version: u16,
}

fn read_request() -> Result<WriteRequest, Failure> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| Failure::new("BadRequest", format!("reading stdin: {e}")))?;
    let request: WriteRequest =
        serde_json::from_str(&text).map_err(|e| Failure::new("BadRequest", format!("invalid request: {e}")))?;
    for (button, target) in &request.buttons {
        if !BUTTONS.contains(&button.as_str()) {
            return Err(Failure::new("BadRequest", format!("unknown button {button:?}")));
        }
        // Whether the target is valid for the button is checked by `Config::apply`.
        if target != "none" && !TARGETS.contains(&target.as_str()) {
            return Err(Failure::new("BadRequest", format!("{button} cannot be mapped to {target:?}")));
        }
    }
    Ok(request)
}

fn backup(dir: &Path, slot: Slot, data: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;
    let time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let path = dir.join(format!("flugdiggi-backup-config-slot{slot}-{time}.bin"));
    std::fs::write(&path, data).map_err(|e| format!("saving the backup {}: {e}", path.display()))?;
    info!("previous config of slot {slot} saved to {}", path.display());
    Ok(())
}

fn write_buttons(port: &UsbPort, slot: Slot, backup_dir: &Path) -> Reply {
    let request = read_request()?;
    let wanted = Named(request.buttons.into_iter().collect::<Vec<_>>());
    let mut c = open(port)?;
    let mut stale = None;
    let result = c.write(Blob::Mapping, slot, Version::New, |old| {
        let mut config = Config::decode(old)?;
        if config.data_version != request.expected_data_version {
            stale = Some(config.data_version);
            return Err("the slot changed since it was read".into());
        }
        // Only the buttons change; `apply` keeps every other byte of the blob.
        config.buttons = wanted;
        backup(backup_dir, slot, old)?;
        Ok(config.apply(old)?)
    });
    if let Some(current) = stale {
        return Err(Failure::new(
            "Stale",
            format!("slot {slot} has data version {current}, expected {}", request.expected_data_version),
        ));
    }
    let (outcome, config) = match result? {
        WriteOutcome::Written(data) => ("written", Config::decode(&data)?),
        WriteOutcome::Unchanged => ("unchanged", c.read_config(slot)?),
    };
    Ok(json!({
        "outcome": outcome,
        "slot": slot.get(),
        "buttons": buttons_json(&config.buttons),
        "data_version": config.data_version,
    }))
}

fn activate(port: &UsbPort, slot: Slot) -> Reply {
    let mut c = open(port)?;
    c.set_active_slot(slot)?;
    Ok(json!({ "active_slot": c.active_slot()?.slot.get() }))
}
