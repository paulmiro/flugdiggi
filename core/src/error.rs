use std::time::Duration;

use crate::types::{Blob, Mac, Slot, UsbPort};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Error type returned by the callback that computes new slot content in [`Controller::write`](crate::Controller::write).
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("USB error while {action}")]
    Usb {
        action: &'static str,
        #[source]
        source: rusb::Error,
    },
    #[error("no Flydigi device (USB id 045e:028e) found")]
    NoDevice,
    #[error("no Flydigi device on USB port {0}")]
    NoDeviceOnPort(UsbPort),
    #[error("no controller with MAC {mac} found{}", answering_list(.answering))]
    NoDeviceWithMac { mac: Mac, answering: Vec<Mac> },
    #[error("several Flydigi devices are connected (USB ports {})", join(.ports))]
    SeveralDevices { ports: Vec<UsbPort> },
    #[error("the USB interface of the device on port {0} is in use by another program")]
    Busy(UsbPort),
    #[error("the controller did not answer command {command:#04x} within {waited:?}")]
    NoReply { command: u8, waited: Duration },
    #[error("the controller stayed unresponsive for {0:?}")]
    Unresponsive(Duration),
    #[error("the controller disappeared and did not come back on USB port {port} within {waited:?}")]
    Disconnected { port: UsbPort, waited: Duration },
    #[error("slot {slot} did not become active within {waited:?}")]
    SwitchTimeout { slot: Slot, waited: Duration },
    #[error("could not read the {blob} of slot {slot}")]
    ReadFailed { blob: Blob, slot: Slot },
    #[error("unsupported {blob} (length {len}, version bytes {version:02x?})")]
    UnsupportedBlob { blob: Blob, len: usize, version: [u8; 2] },
    #[error("new {blob} has {got} bytes, expected {expected}")]
    WrongLength { blob: Blob, expected: usize, got: usize },
    #[error("the controller kept ignoring writes to slot {0}")]
    WriteIgnored(Slot),
    #[error("writing packet {packet} to slot {slot} failed: {reason}")]
    WriteFailed { slot: Slot, packet: usize, reason: String },
    #[error("verification of slot {slot} failed after repairing: {reason}")]
    VerifyFailed { slot: Slot, reason: String },
    #[error("slot {0} was no longer active before saving, nothing was saved")]
    SlotChanged(Slot),
    #[error("invalid slot {0} (valid: 0-7)")]
    InvalidSlot(u8),
    #[error("slot {0} is a Switch-mode slot; only slots 0-3 can be made active")]
    NotSelectable(Slot),
    #[error("invalid MAC {0:?} (expected 4 hex bytes like 38:33:FD:F5)")]
    InvalidMac(String),
    #[error("invalid USB port {0:?} (expected bus-port path like 3-1.2)")]
    InvalidPort(String),
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("command {0:02x?} is not on the read-only allowlist")]
    NotAllowed(Vec<u8>),
    #[error("computing the new {0} failed")]
    Prepare(Blob, #[source] BoxError),
    #[error("internal error: {0} (this is a bug)")]
    Internal(&'static str),
}

impl Error {
    pub(crate) fn usb(action: &'static str) -> impl FnOnce(rusb::Error) -> Error {
        move |source| Error::Usb { action, source }
    }
}

fn join(items: &[impl std::fmt::Display]) -> String {
    items.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
}

fn answering_list(macs: &[Mac]) -> String {
    if macs.is_empty() { String::new() } else { format!(" (answering: {})", join(macs)) }
}
