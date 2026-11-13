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
    handle: DeviceHandle<GlobalContext>,
    pub product: String,
    pub trace: bool,
}

impl Transport {
    /// Opens the first 045e:028e device whose manufacturer string is "Flydigi".
    /// The kernel driver (xpad) is detached while the transport is alive and reattached on drop.
    pub fn open() -> Result<Self> {
        for dev in rusb::devices()?.iter() {
            let desc = dev.device_descriptor()?;
            if desc.vendor_id() != XUSB_VID || desc.product_id() != XUSB_PID {
                continue;
            }
            let handle = dev.open().context("opening USB device")?;
            let manufacturer = handle.read_manufacturer_string_ascii(&desc).unwrap_or_default();
            if !manufacturer.eq_ignore_ascii_case("flydigi") {
                continue;
            }
            let product = handle.read_product_string_ascii(&desc).unwrap_or_default();
            handle.set_auto_detach_kernel_driver(true)?;
            handle.claim_interface(IFACE).context("claiming interface 0")?;
            let t = Transport { handle, product, trace: false };
            t.drain();
            return Ok(t);
        }
        bail!("no Flydigi device (045e:028e, manufacturer \"Flydigi\") found")
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        if self.trace {
            eprintln!(">> {}", hex(data));
        }
        self.handle.write_interrupt(EP_OUT, data, Duration::from_millis(500))?;
        Ok(())
    }

    /// Returns the next IN packet, or None on timeout.
    pub fn read(&self, timeout: Duration) -> Result<Option<Packet>> {
        let mut buf = [0u8; 64];
        match self.handle.read_interrupt(EP_IN, &mut buf, timeout) {
            Ok(n) => {
                let mut p = [0u8; 32];
                let n = n.min(32);
                p[..n].copy_from_slice(&buf[..n]);
                Ok(Some(p))
            }
            Err(rusb::Error::Timeout) => Ok(None),
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
                    eprintln!("<< {}", hex(&p));
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
        let _ = self.handle.release_interface(IFACE);
    }
}

/// Replies to old-protocol commands carry a 0xA5/0x5A marker at byte 14; everything else is an input report.
pub fn is_reply(p: &Packet) -> bool {
    p[14] == 0xa5 || p[14] == 0x5a
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}
