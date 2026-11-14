use std::cell::RefCell;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
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

/// Opens the first 045e:028e device whose manufacturer is "Flydigi" or whose product contains "Flydigi".
fn find() -> Result<Option<(DeviceHandle<GlobalContext>, String)>> {
    for dev in rusb::devices()?.iter() {
        let desc = dev.device_descriptor()?;
        if desc.vendor_id() != XUSB_VID || desc.product_id() != XUSB_PID {
            continue;
        }
        let handle = dev.open().context("opening USB device")?;
        // A wired Vader 4 Pro stalls the manufacturer string request.
        let manufacturer = handle.read_manufacturer_string_ascii(&desc).unwrap_or_default();
        let product = handle.read_product_string_ascii(&desc).unwrap_or_default();
        if !manufacturer.eq_ignore_ascii_case("flydigi") && !product.to_ascii_lowercase().contains("flydigi") {
            continue;
        }
        handle.set_auto_detach_kernel_driver(true)?;
        handle.claim_interface(IFACE).context("claiming interface 0")?;
        return Ok(Some((handle, product)));
    }
    Ok(None)
}

impl Transport {
    /// The kernel driver (xpad) is detached while the transport is alive and reattached on drop.
    pub fn open() -> Result<Self> {
        let Some((handle, product)) = find()? else {
            bail!("no Flydigi device (045e:028e) found");
        };
        let t = Transport { handle: RefCell::new(handle), product, trace: false, last_sent: Default::default(), start: Instant::now() };
        t.drain();
        Ok(t)
    }

    /// True if the device behind the handle has disappeared from the bus (e.g. re-enumerated).
    pub fn is_gone(&self) -> bool {
        let dev = self.handle.borrow().device();
        let Ok(list) = rusb::devices() else { return false };
        !list.iter().any(|d| d.bus_number() == dev.bus_number() && d.address() == dev.address())
    }

    /// Waits for the device to come back after a re-enumeration and reopens it.
    pub fn reconnect(&self, max: Duration) -> Result<()> {
        let start = Instant::now();
        loop {
            if let Ok(Some((handle, _))) = find() {
                let _ = self.handle.borrow().release_interface(IFACE);
                *self.handle.borrow_mut() = handle;
                self.last_sent.borrow_mut().clear();
                if self.trace {
                    eprintln!("{:6} -- reconnected", self.start.elapsed().as_millis());
                }
                self.drain();
                return Ok(());
            }
            if start.elapsed() > max {
                bail!("device did not come back within {max:?}");
            }
            std::thread::sleep(Duration::from_millis(250));
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
