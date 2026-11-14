use std::cell::RefCell;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rusb::{DeviceHandle, GlobalContext};

/// Flydigi dongles (and wired old-protocol controllers) pose as an Xbox 360 controller.
pub const XUSB_VID: u16 = 0x045e;
pub const XUSB_PID: u16 = 0x028e;

const IFACE: u8 = 0;
const EP_IN: u8 = 0x81;
const EP_OUT: u8 = 0x05;

pub type Packet = [u8; 32];

pub struct Transport {
    handle: RefCell<DeviceHandle<GlobalContext>>,
    pub product: String,
    pub trace: bool,
    last_sent: RefCell<Vec<u8>>,
    start: Instant,
}

/// A 045e:028e device whose manufacturer is "Flydigi" or whose product contains "Flydigi".
pub struct Candidate {
    dev: rusb::Device<GlobalContext>,
    pub product: String,
    /// Guessed without claiming the device: a wired Vader 4 Pro stalls the manufacturer string request,
    /// the dongle answers it.
    pub wired: bool,
}

impl Candidate {
    pub fn port(&self) -> String {
        port(&self.dev)
    }
}

/// The USB port path as in sysfs (e.g. "3-1.2"). Unlike the address, it survives a re-enumeration.
fn port(dev: &rusb::Device<GlobalContext>) -> String {
    let ports = dev.port_numbers().unwrap_or_default();
    let path = ports.iter().map(u8::to_string).collect::<Vec<_>>().join(".");
    format!("{}-{path}", dev.bus_number())
}

/// Lists Flydigi devices. Devices that can't be opened (e.g. no permission) are skipped.
pub fn candidates() -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
    for dev in rusb::devices()?.iter() {
        let Ok(desc) = dev.device_descriptor() else { continue };
        if desc.vendor_id() != XUSB_VID || desc.product_id() != XUSB_PID {
            continue;
        }
        let Ok(handle) = dev.open() else { continue };
        // A wired Vader 4 Pro stalls the manufacturer string request.
        let manufacturer = handle.read_manufacturer_string_ascii(&desc);
        let wired = manufacturer.is_err();
        let manufacturer = manufacturer.unwrap_or_default();
        let product = handle.read_product_string_ascii(&desc).unwrap_or_default();
        if manufacturer.eq_ignore_ascii_case("flydigi") || product.to_ascii_lowercase().contains("flydigi") {
            out.push(Candidate { dev, product, wired });
        }
    }
    Ok(out)
}

impl Transport {
    /// The kernel driver (xpad) is detached while the transport is alive and reattached on drop.
    pub fn open(cand: &Candidate) -> Result<Self> {
        let handle = cand.dev.open().context("opening USB device")?;
        handle.set_auto_detach_kernel_driver(true)?;
        handle.claim_interface(IFACE).context("claiming interface 0 (in use by another program?)")?;
        let t = Transport {
            handle: RefCell::new(handle),
            product: cand.product.clone(),
            trace: false,
            last_sent: Default::default(),
            start: Instant::now(),
        };
        t.drain();
        Ok(t)
    }

    /// True if the device behind the handle has disappeared from the bus (e.g. re-enumerated).
    pub fn is_gone(&self) -> bool {
        let dev = self.handle.borrow().device();
        let Ok(list) = rusb::devices() else { return false };
        !list.iter().any(|d| d.bus_number() == dev.bus_number() && d.address() == dev.address())
    }

    pub fn port(&self) -> String {
        port(&self.handle.borrow().device())
    }

    /// Continues on the device of `other` (after a re-enumeration); `other` gets the old handle.
    pub fn take_over(&self, other: &Transport) {
        std::mem::swap(&mut *self.handle.borrow_mut(), &mut *other.handle.borrow_mut());
        std::mem::swap(&mut *self.last_sent.borrow_mut(), &mut *other.last_sent.borrow_mut());
        if self.trace {
            eprintln!("{:6} -- reconnected", self.start.elapsed().as_millis());
        }
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        if self.trace {
            eprintln!("{:6} >> {}", self.start.elapsed().as_millis(), hex(data));
        }
        self.handle.borrow().write_interrupt(EP_OUT, data, Duration::from_millis(500))?;
        *self.last_sent.borrow_mut() = data.to_vec();
        Ok(())
    }

    /// Returns the next IN packet, or None on timeout.
    pub fn read(&self, timeout: Duration) -> Result<Option<Packet>> {
        let mut buf = [0u8; 64];
        let r = self.handle.borrow().read_interrupt(EP_IN, &mut buf, timeout);
        match r {
            Ok(n) => {
                let mut p = [0u8; 32];
                let n = n.min(32);
                p[..n].copy_from_slice(&buf[..n]);
                Ok(Some(p))
            }
            Err(rusb::Error::Timeout) => Ok(None),
            // Occasional transient I/O errors were seen on the dongle; treat them like a missed packet.
            Err(rusb::Error::Io) => {
                std::thread::sleep(Duration::from_millis(50));
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Reads until `pred` matches or `timeout` elapses.
    pub fn read_until(&self, timeout: Duration, mut pred: impl FnMut(&Packet) -> bool) -> Result<Option<Packet>> {
        let deadline = Instant::now() + timeout;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            if let Some(p) = self.read(deadline - now)? {
                if self.trace && is_reply(&p) {
                    eprintln!("{:6} << {}", self.start.elapsed().as_millis(), hex(&p));
                }
                if pred(&p) {
                    return Ok(Some(p));
                }
            }
        }
    }

    /// Discards stale packets. Input reports stream continuously, so this is bounded by time.
    fn drain(&self) {
        let deadline = Instant::now() + Duration::from_millis(50);
        while Instant::now() < deadline {
            if !matches!(self.read(Duration::from_millis(10)), Ok(Some(_))) {
                break;
            }
        }
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        let _ = self.handle.borrow().release_interface(IFACE);
    }
}

/// Replies to old-protocol commands carry a 0xA5/0x5A marker at byte 14; everything else is an input report.
pub fn is_reply(p: &Packet) -> bool {
    p[14] == 0xa5 || p[14] == 0x5a
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

impl Transport {
    /// The dongle silently drops an OUT packet identical to the previous one.
    pub fn would_repeat(&self, data: &[u8]) -> bool {
        *self.last_sent.borrow() == data
    }

    pub fn last_sent(&self) -> Vec<u8> {
        self.last_sent.borrow().clone()
    }
}
