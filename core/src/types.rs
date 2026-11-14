use std::fmt;
use std::str::FromStr;

use crate::error::Error;

/// A config slot: 0-3 are used in `XInput` mode (dongle and cable), 4-7 in Nintendo Switch mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Slot(u8);

impl Slot {
    pub const MAX: u8 = 7;

    pub const fn new(n: u8) -> Result<Self, Error> {
        if n <= Self::MAX { Ok(Slot(n)) } else { Err(Error::InvalidSlot(n)) }
    }

    pub const fn get(self) -> u8 {
        self.0
    }

    pub const fn is_switch_mode(self) -> bool {
        self.0 >= 4
    }

    /// Some other `XInput` slot, used to switch away and back.
    pub(crate) const fn away(self) -> Slot {
        if self.0 == 0 { Slot(1) } else { Slot(0) }
    }
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for Slot {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        s.parse::<u8>().map_err(|_| Error::InvalidSlot(u8::MAX)).and_then(Slot::new)
    }
}

impl TryFrom<u8> for Slot {
    type Error = Error;
    fn try_from(n: u8) -> Result<Self, Error> {
        Slot::new(n)
    }
}

/// The 4 bytes of the controller's MAC that its heartbeat reports, e.g. `38:33:FD:F5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Mac(pub [u8; 4]);

impl fmt::Display for Mac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.0;
        write!(f, "{a:02X}:{b:02X}:{c:02X}:{d:02X}")
    }
}

impl FromStr for Mac {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        let bad = || Error::InvalidMac(s.to_owned());
        let bytes: Vec<u8> = s
            .split(':')
            .map(|b| if b.len() == 2 { u8::from_str_radix(b, 16).map_err(|_| bad()) } else { Err(bad()) })
            .collect::<Result<_, _>>()?;
        bytes.try_into().map(Mac).map_err(|_| bad())
    }
}

/// A USB port path as in sysfs (e.g. `3-1.2`). Unlike the device address it survives a re-enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UsbPort(String);

impl UsbPort {
    pub(crate) fn of(dev: &rusb::Device<rusb::GlobalContext>) -> Self {
        let ports = dev.port_numbers().unwrap_or_default();
        let path = ports.iter().map(u8::to_string).collect::<Vec<_>>().join(".");
        UsbPort(format!("{}-{path}", dev.bus_number()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UsbPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for UsbPort {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        let numbers =
            |p: &str| !p.is_empty() && p.split('.').all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        match s.split_once('-') {
            Some((bus, path)) if numbers(bus) && !bus.contains('.') && numbers(path) => Ok(UsbPort(s.to_owned())),
            _ => Err(Error::InvalidPort(s.to_owned())),
        }
    }
}

/// Which device [`Controller::open`](crate::Controller::open) picks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Selector {
    /// The only connected device; fails if there are several.
    #[default]
    Only,
    /// The device on this USB port. Touches no other device.
    Port(UsbPort),
    /// The controller reporting this MAC. Every device has to be claimed to read its MAC, so the other
    /// controllers briefly disconnect from their kernel driver (games see them disconnect and reconnect).
    Mac(Mac),
}

impl FromStr for Selector {
    type Err = Error;
    /// A value containing `:` is a MAC, anything else a USB port.
    fn from_str(s: &str) -> Result<Self, Error> {
        if s.contains(':') { s.parse().map(Selector::Mac) } else { s.parse().map(Selector::Port) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    /// The 2.4 GHz USB dongle.
    Dongle,
    /// A USB cable.
    Cable,
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Connection::Dongle => "dongle",
            Connection::Cable => "cable",
        })
    }
}

/// Firmware version, e.g. 6.9.5.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Firmware(pub [u8; 4]);

impl fmt::Display for Firmware {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.0;
        write!(f, "{a}.{b}.{c}.{d}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Battery {
    /// 0 (lowest) to 5.
    Level(u8),
    Charging,
}

impl fmt::Display for Battery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Battery::Level(l) => write!(f, "{l}/5"),
            Battery::Charging => f.write_str("charging"),
        }
    }
}

/// The controller's heartbeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// 0x55 / 0x5B for the Vader 4 family.
    pub device_type: u8,
    pub mac: Mac,
    pub firmware: Firmware,
    pub battery: Battery,
    pub chip_type: u8,
    pub connection: Connection,
    pub motion_chip: u8,
    /// Config id byte of the heartbeat; not always the active slot.
    pub config_id: u8,
}

/// A connected Flydigi USB device, as listed by [`Controller::list`](crate::Controller::list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSummary {
    pub port: UsbPort,
    pub product: String,
    pub connection: Connection,
    /// True if `connection` is guessed from the USB descriptors: a wired Vader 4 Pro (firmware 6.9.5.5)
    /// stalls the manufacturer string request. This firmware quirk may not hold for other firmware or models.
    pub connection_guessed: bool,
    /// The heartbeat, if probed and answered.
    pub info: Option<DeviceInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveSlot {
    pub slot: Slot,
    /// The slot used in Switch mode, if reported.
    pub switch_mode_slot: Option<Slot>,
}

/// The per-slot data blobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Blob {
    /// Button mapping, sticks, triggers, rumble, motion, macros ([`config::CONFIG_LEN`](crate::config::CONFIG_LEN) bytes).
    Mapping,
    /// LED colours and effects ([`led::LED_LEN`](crate::led::LED_LEN) bytes).
    Led,
}

impl Blob {
    #[allow(clippy::len_without_is_empty)]
    pub const fn len(self) -> usize {
        match self {
            Blob::Mapping => crate::config::CONFIG_LEN,
            Blob::Led => crate::led::LED_LEN,
        }
    }
}

impl fmt::Display for Blob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Blob::Mapping => "mapping config",
            Blob::Led => "LED config",
        })
    }
}

/// The data version (random id the official app changes on every save) a write saves with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// A new random version, like the official app.
    New,
    /// The slot's current version.
    Keep,
    Exact(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    /// The slot already had this content; nothing was written.
    Unchanged,
    /// Saved and verified. Holds the blob as stored (a mapping config gets the new data version).
    Written(Vec<u8>),
}

/// A reply to [`Controller::query_raw`](crate::Controller::query_raw).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawReply {
    pub after: std::time::Duration,
    pub packet: [u8; 32],
}
