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
        let c = Controller { t: Transport::open()? };
        // The last reply of a previous session is unknown; after these two, the last reply is a known heartbeat.
        c.separator(0x20)?;
        c.separator(0x10)?;
        Ok(c)
    }

    /// The controller does not answer when the reply would be identical to its previous reply
    /// (e.g. the same command twice in a row), so a different command is sent in between.
    fn separate(&self, req: &[u8]) -> Result<()> {
        self.separator(if req[1] == 0x10 { 0x20 } else { 0x10 })
    }

    fn separator(&self, id: u8) -> Result<()> {
        let req = if id == 0x10 { cmd(0x10, &[self.nonce()]) } else { cmd(id, &[]) };
        self.t.write(&req)?;
        self.t.read_until(Duration::from_millis(150), |p| is_reply(p) && p[15] == id)?;
        Ok(())
    }

    fn nonce(&self) -> u8 {
        let last = self.t.last_sent();
        loop {
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as u8)
                .unwrap_or(0x5c);
            if last.get(1) != Some(&0x10) || last.get(2) != Some(&n) {
                return n;
            }
        }
    }

    /// Writes `req` without waiting for a reply (for exploration).
    pub fn send_raw(&self, req: &[u8]) -> Result<()> {
        self.send(req)
    }

    /// Writes `req`, first separating it from an identical previous packet.
    fn send(&self, req: &[u8]) -> Result<()> {
        let last = self.t.last_sent();
        let same_cmd = last.get(1) == req.get(1)
            && (req[1] != 0x50 || last.get(2) == req.get(2))
            && !matches!(req[1], 0x21 | 0x24);
        if same_cmd || self.t.would_repeat(req) {
            self.separate(req)?;
        }
        self.t.write(req)
    }

    /// Sends `req` and returns the first reply matching `ack`, retrying on timeout.
    pub fn request(&self, req: &[u8], ack: impl Fn(&Packet) -> bool) -> Result<Packet> {
        self.request_timeout(req, TIMEOUT, ack)
    }

    pub fn request_timeout(&self, req: &[u8], timeout: Duration, ack: impl Fn(&Packet) -> bool) -> Result<Packet> {
        for _ in 0..RETRIES {
            self.send(req)?;
            if let Some(p) = self.t.read_until(timeout, |p| is_reply(p) && ack(p))? {
                return Ok(p);
            }
        }
        bail!("no reply to command 0x{:02x} after {RETRIES} tries", req[1])
    }

    pub fn heartbeat(&self) -> Result<DeviceInfo> {
        let d = self.request(&cmd(0x10, &[self.nonce()]), |p| p[15] == 0x10)?;
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
        // The controller does not answer a switch to the slot that is already active.
        if self.current_config_id()?.0 == slot {
            return Ok(());
        }
        // The switch reply ([17] = 1) is unreliable: it sometimes arrives late or not at all although the switch
        // happened. So success is judged by polling the active slot.
        let switch = cmd(0x50, &[5, slot]);
        let start = std::time::Instant::now();
        let mut resent = false;
        self.send(&switch)?;
        while start.elapsed() < Duration::from_secs(5) {
            if !resent && start.elapsed() > Duration::from_millis(1500) {
                self.send(&switch)?;
                resent = true;
            }
            self.send(&cmd(0x20, &[]))?;
            if let Some(d) = self.t.read_until(Duration::from_millis(200), |p| is_reply(p) && p[15] == 0x20)? {
                if d[16] == slot {
                    if self.t.trace {
                        eprintln!("slot {slot} active after {:?}", start.elapsed());
                    }
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        bail!("slot {slot} did not become active within 5 s")
    }

    /// Raw mapping config blob of `slot` (multi-packet reply to command 0x21).
    pub fn read_mapping_config_raw(&self, slot: u8) -> Result<Vec<u8>> {
        'retry: for _ in 0..RETRIES {
            self.send(&cmd(0x21, &[slot]))?;
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

    /// Writes consecutive 10-byte packets of a mapping config, starting at packet `start`.
    fn write_mapping_packets(&self, slot: u8, start: u8, packets: &[&[u8]]) -> Result<()> {
        self.request(&cmd(0x23, &[start, packets.len() as u8, 0xa0, slot]), |p| p[15] == 0x23)?;
        for (i, data) in packets.iter().enumerate() {
            let mut args = [0u8; 12];
            args[..10].copy_from_slice(data);
            args[10] = 0xa0;
            args[11] = i as u8;
            self.request(&cmd(0x24, &args), |p| p[15] == 0x24)?;
        }
        Ok(())
    }

    /// Writes only the 10-byte packets in which `new` differs from `old`. Returns the number of packets written.
    pub fn write_mapping_config_diff(&self, slot: u8, old: &[u8], new: &[u8]) -> Result<usize> {
        if old.len() != new.len() || old.len() % 10 != 0 {
            bail!("config blobs must have equal length that is a multiple of 10");
        }
        let old: Vec<&[u8]> = old.chunks(10).collect();
        let new: Vec<&[u8]> = new.chunks(10).collect();
        let mut written = 0;
        let mut i = 0;
        while i < new.len() {
            if old[i] == new[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < new.len() && old[i] != new[i] {
                i += 1;
            }
            self.write_mapping_packets(slot, start as u8, &new[start..i])?;
            written += i - start;
        }
        Ok(written)
    }
}

impl Controller {
    /// Persists the active slot's working copy to flash and sets its data version
    /// (the official app picks a new random version on every save).
    pub fn save_active_config(&self, version: u16) -> Result<()> {
        let [lo, hi] = version.to_le_bytes();
        self.request_timeout(&cmd(0x50, &[3, lo, hi]), Duration::from_secs(10), |p| p[15] == 0x50 && p[16] == 3)?;
        Ok(())
    }
}

impl Controller {
    /// Writes a complete config blob with the "write all" start command (0x25). The official app uses
    /// this to persist configs of old-protocol controllers.
    pub fn write_mapping_config_full(&self, slot: u8, data: &[u8]) -> Result<()> {
        if data.len() % 10 != 0 {
            bail!("config length must be a multiple of 10");
        }
        let packets: Vec<&[u8]> = data.chunks(10).collect();
        self.request(&cmd(0x25, &[packets.len() as u8, 0xa0, slot]), |p| p[15] == 0x25)?;
        for (i, chunk) in packets.iter().enumerate() {
            let mut args = [0u8; 12];
            args[..10].copy_from_slice(chunk);
            args[10] = 0xa0;
            args[11] = i as u8;
            self.request(&cmd(0x24, &args), |p| p[15] == 0x24)?;
        }
        Ok(())
    }
}

impl Controller {
    /// Reads a slot's config. Reading activates the slot on the controller, so the previously
    /// active slot is restored afterwards.
    pub fn read_config(&self, slot: u8) -> Result<Vec<u8>> {
        let active = self.current_config_id()?.0;
        let data = self.read_mapping_config_raw(slot);
        self.apply_config(active)?;
        data
    }

    /// Persistently writes a complete config to `slot`, verifies it by reading it back and restores
    /// the previously active slot.
    pub fn write_config(&self, slot: u8, data: &[u8]) -> Result<()> {
        if data.len() != crate::config::CONFIG_LEN {
            bail!("config must be {} bytes", crate::config::CONFIG_LEN);
        }
        let active = self.current_config_id()?.0;
        self.write_mapping_config_full(slot, data)?;
        let back = self.read_mapping_config_raw(slot);
        self.apply_config(active)?;
        if back? != data {
            bail!("read-back of slot {slot} differs from what was written");
        }
        Ok(())
    }
}
