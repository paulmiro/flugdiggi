//! Old ("XInput") Flydigi protocol, as used by the Vader 4 Pro dongle.
//!
//! Requests are 15 bytes `[0xA5, cmd, args...]` written to the XUSB OUT endpoint.
//! Replies arrive interleaved with input reports and are 32-byte packets with
//! `[14] = 0xA5|0x5A` and `[15]` = the command id the reply belongs to.

use std::time::Duration;

use anyhow::{Result, bail};

use crate::transport::{Packet, Transport, is_reply};

const PREFIX: u8 = 0xa5;
const TIMEOUT: Duration = Duration::from_millis(500);
const RETRIES: usize = 3;

pub struct Controller {
    pub t: Transport,
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub device_type: u8,
    pub mac: String,
    pub firmware: String,
    pub battery: u8,
    pub charging: bool,
    pub chip_type: u8,
    pub wired: bool,
    pub motion_chip: u8,
    pub current_config: u8,
}

fn cmd(id: u8, args: &[u8]) -> [u8; 15] {
    let mut p = [0u8; 15];
    p[0] = PREFIX;
    p[1] = id;
    p[2..2 + args.len()].copy_from_slice(args);
    p
}

fn fw(lo: u8, hi: u8) -> String {
    format!("{}.{}.{}.{}", hi >> 4, hi & 0xf, lo >> 4, lo & 0xf)
}

impl Controller {
    pub fn open() -> Result<Self> {
        Ok(Controller { t: Transport::open()? })
    }

    /// Sends `req` and returns the first reply matching `ack`, retrying on timeout.
    pub fn request(&self, req: &[u8], ack: impl Fn(&Packet) -> bool) -> Result<Packet> {
        for _ in 0..RETRIES {
            self.t.write(req)?;
            if let Some(p) = self.t.read_until(TIMEOUT, |p| is_reply(p) && ack(p))? {
                return Ok(p);
            }
        }
        bail!("no reply to command 0x{:02x} after {RETRIES} tries", req[1])
    }

    pub fn heartbeat(&self) -> Result<DeviceInfo> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .subsec_nanos() as u8;
        let d = self.request(&cmd(0x10, &[nonce]), |p| p[15] == 0x10)?;
        Ok(DeviceInfo {
            device_type: d[16],
            mac: format!("{:02X}:{:02X}:{:02X}:{:02X}", d[20], d[19], d[18], d[17]),
            firmware: fw(d[21], d[22]),
            battery: if d[23] >> 4 == 1 { 6 } else { d[23] & 0xf },
            charging: d[23] >> 4 == 1,
            chip_type: d[24],
            wired: d[25] == 1,
            motion_chip: d[26],
            current_config: d[27],
        })
    }

    /// Returns (active config slot, active Switch-mode slot if reported).
    pub fn current_config_id(&self) -> Result<(u8, Option<u8>)> {
        let d = self.request(&cmd(0x20, &[]), |p| p[15] == 0x20)?;
        Ok((d[16], (d[17] == 0xa0).then_some(d[18])))
    }

    /// Per-slot random ids that change whenever a slot's mapping is rewritten.
    pub fn config_versions(&self) -> Result<[u16; 4]> {
        let d = self.request(&cmd(0x50, &[4]), |p| p[15] == 0x50 && p[16] == 4)?;
        Ok(std::array::from_fn(|i| u16::from_le_bytes([d[17 + 2 * i], d[18 + 2 * i]])))
    }

    /// Makes `slot` the active config.
    pub fn apply_config(&self, slot: u8) -> Result<()> {
        let d = self.request(&cmd(0x50, &[5, slot]), |p| p[15] == 0x50 && p[16] == 5)?;
        if d[17] != 0 {
            bail!("controller rejected switching to slot {slot} (status {})", d[17]);
        }
        Ok(())
    }

    /// Raw mapping config blob of `slot` (multi-packet reply to command 0x21).
    pub fn read_mapping_config_raw(&self, slot: u8) -> Result<Vec<u8>> {
        'retry: for _ in 0..RETRIES {
            self.t.write(&cmd(0x21, &[slot]))?;
            let mut count: Option<usize> = None;
            let mut data: Vec<u8> = Vec::new();
            let mut received: Vec<bool> = Vec::new();
            loop {
                let Some(p) = self.t.read_until(TIMEOUT, |p| is_reply(p) && p[15] == 0x22)? else {
                    continue 'retry;
                };
                let idx = p[16] as usize;
                if count.is_none() {
                    if idx != 0 {
                        continue;
                    }
                    let n = match (p[18], p[17]) {
                        (3, 0) => 79,
                        (3, 1) => 84,
                        _ => p[19] as usize,
                    };
                    count = Some(n);
                    data = vec![0; 10 * n];
                    received = vec![false; n];
                }
                let n = count.unwrap();
                if idx < n {
                    data[10 * idx..10 * idx + 10].copy_from_slice(&p[17..27]);
                    received[idx] = true;
                }
                if idx + 1 == n {
                    if received.iter().all(|&r| r) {
                        return Ok(data);
                    }
                    let missing = received.iter().filter(|&&r| !r).count();
                    eprintln!("slot {slot}: {missing} packets missing, retrying");
                    continue 'retry;
                }
            }
        }
        bail!("could not read mapping config of slot {slot}")
    }
}
