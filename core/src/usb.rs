//! USB transport: the vendor interface of the XUSB device (`045e:028e`).

use std::time::{Duration, Instant};

use rusb::{Device, DeviceHandle, GlobalContext};
use tracing::{debug, warn};

use crate::bytes::hex;
use crate::error::{Error, Result};
use crate::protocol::{Packet, Request, is_reply};
use crate::types::UsbPort;

/// Flydigi dongles (and wired old-protocol controllers) pose as an Xbox 360 controller.
const XUSB_VID: u16 = 0x045e;
const XUSB_PID: u16 = 0x028e;
const IFACE: u8 = 0;
const EP_IN: u8 = 0x81;
const EP_OUT: u8 = 0x05;
const WRITE_TIMEOUT: Duration = Duration::from_millis(500);

/// A 045e:028e device whose manufacturer is "Flydigi" or whose product contains "Flydigi".
#[derive(Debug)]
pub(crate) struct Candidate {
    dev: Device<GlobalContext>,
    pub port: UsbPort,
    pub product: String,
    /// Guessed without claiming the device: a wired Vader 4 Pro stalls the manufacturer string
    /// request, the dongle answers it.
    pub wired_guess: bool,
}

/// Lists Flydigi devices. Devices that can't be opened (e.g. no permission) are skipped.
pub(crate) fn candidates() -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
    for dev in rusb::devices().map_err(Error::usb("listing USB devices"))?.iter() {
        let Ok(desc) = dev.device_descriptor() else { continue };
        if desc.vendor_id() != XUSB_VID || desc.product_id() != XUSB_PID {
            continue;
        }
        let port = UsbPort::of(&dev);
        let handle = match dev.open() {
            Ok(h) => h,
            Err(e) => {
                warn!("skipping the Xbox 360 type device on USB port {port}: cannot open it ({e})");
                continue;
            }
        };
        let manufacturer = handle.read_manufacturer_string_ascii(&desc);
        let wired_guess = manufacturer.is_err();
        let manufacturer = manufacturer.unwrap_or_default();
        let product = handle.read_product_string_ascii(&desc).unwrap_or_default();
        if manufacturer.eq_ignore_ascii_case("flydigi") || product.to_ascii_lowercase().contains("flydigi") {
            debug!(%port, product, wired_guess, "found Flydigi device");
            out.push(Candidate { dev, port, product, wired_guess });
        } else {
            debug!(%port, manufacturer, product, "ignoring non-Flydigi device");
        }
    }
    Ok(out)
}

#[derive(Debug)]
pub(crate) struct Transport {
    handle: DeviceHandle<GlobalContext>,
    port: UsbPort,
    product: String,
    last_sent: Option<Request>,
}

impl Transport {
    /// The kernel driver (xpad) is detached while the transport is alive and reattached on drop.
    pub fn open(cand: &Candidate) -> Result<Self> {
        let handle = cand.dev.open().map_err(Error::usb("opening the USB device"))?;
        match handle.set_auto_detach_kernel_driver(true) {
            Ok(()) | Err(rusb::Error::NotSupported) => {}
            Err(e) => return Err(Error::usb("enabling kernel driver detaching")(e)),
        }
        handle.claim_interface(IFACE).map_err(|e| match e {
            rusb::Error::Busy => Error::Busy(cand.port.clone()),
            e => Error::usb("claiming the USB interface")(e),
        })?;
        debug!(port = %cand.port, "claimed USB interface");
        let t = Transport { handle, port: cand.port.clone(), product: cand.product.clone(), last_sent: None };
        t.drain();
        Ok(t)
    }

    /// True if the device has disappeared from the bus (e.g. re-enumerated).
    pub fn is_gone(&self) -> bool {
        let dev = self.handle.device();
        let Ok(list) = rusb::devices() else { return false };
        !list.iter().any(|d| d.bus_number() == dev.bus_number() && d.address() == dev.address())
    }

    pub fn port(&self) -> &UsbPort {
        &self.port
    }

    pub fn product(&self) -> &str {
        &self.product
    }

    pub fn last_sent(&self) -> Option<&Request> {
        self.last_sent.as_ref()
    }

    pub fn write(&mut self, data: &Request) -> Result<()> {
        debug!(">> {}", hex(data));
        self.handle.write_interrupt(EP_OUT, data, WRITE_TIMEOUT).map_err(Error::usb("sending a command"))?;
        self.last_sent = Some(*data);
        Ok(())
    }

    /// Returns the next IN packet, or None on timeout.
    pub fn read(&self, timeout: Duration) -> Result<Option<Packet>> {
        let mut buf = [0u8; 64];
        match self.handle.read_interrupt(EP_IN, &mut buf, timeout) {
            Ok(_) => {
                let mut p = [0u8; 32];
                p.copy_from_slice(&buf[..32]);
                Ok(Some(p))
            }
            Err(rusb::Error::Timeout) => Ok(None),
            Err(rusb::Error::Io) => {
                warn!("transient USB I/O error while reading; treating it as a missed packet");
                std::thread::sleep(Duration::from_millis(50));
                Ok(None)
            }
            Err(e) => Err(Error::usb("reading a reply")(e)),
        }
    }

    /// Reads until `pred` matches or `timeout` elapses.
    pub fn read_until(&self, timeout: Duration, mut pred: impl FnMut(&Packet) -> bool) -> Result<Option<Packet>> {
        let start = Instant::now();
        loop {
            let left = timeout.saturating_sub(start.elapsed());
            if left.is_zero() {
                return Ok(None);
            }
            if let Some(p) = self.read(left)? {
                if is_reply(&p) {
                    debug!("<< {}", hex(&p));
                }
                if pred(&p) {
                    return Ok(Some(p));
                }
            }
        }
    }

    /// Discards stale packets. Input reports stream continuously, so this is bounded by time.
    fn drain(&self) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(50) {
            if !matches!(self.read(Duration::from_millis(10)), Ok(Some(_))) {
                break;
            }
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        if let Err(e) = self.handle.release_interface(IFACE) {
            debug!(port = %self.port, "releasing the USB interface failed: {e}");
        }
    }
}
