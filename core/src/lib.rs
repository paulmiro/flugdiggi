//! Communication with Flydigi controllers that speak the old (`XInput`) protocol of Flydigi Space
//! Station, such as the Vader 4 Pro through its 2.4 GHz dongle or a USB cable.
//!
//! ```no_run
//! use flugdiggi_core::{Blob, Controller, Selector, Slot, Version};
//!
//! # fn main() -> flugdiggi_core::Result<()> {
//! let mut c = Controller::open(&Selector::Only)?;
//! let slot = Slot::new(1)?;
//! let mut config = c.read_config(slot)?;
//! config.name = "Racing".into();
//! c.write(Blob::Mapping, slot, Version::New, |old| Ok(config.apply(old)?))?;
//! # Ok(())
//! # }
//! ```
//!
//! The library logs through `tracing`: `warn` for recovered problems (re-enumerations, ignored or
//! repaired writes), `debug` for protocol details including every packet.

mod bytes;
pub mod config;
mod controller;
mod error;
pub mod led;
mod protocol;
mod transfer;
mod types;
mod usb;

pub use controller::Controller;
pub use error::{BoxError, Error, Result};
pub use types::{
    ActiveSlot, Battery, Blob, Connection, DeviceInfo, DeviceSummary, Firmware, Mac, RawReply, Selector, Slot, UsbPort,
    Version, WriteOutcome,
};

const _: () = {
    const fn is_send<T: Send>() {}
    is_send::<Controller>();
};
