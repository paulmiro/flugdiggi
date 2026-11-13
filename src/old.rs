//! Old ("XInput") Flydigi protocol, as used by the Vader 4 Pro dongle.
//!
//! Requests are 15 bytes `[0xA5, cmd, args...]` written to the XUSB OUT endpoint. Replies arrive
//! interleaved with input reports as 32-byte packets with `[14] = 0xA5|0x5A` and `[15]` = reply id.
//!
//! Quirks observed on hardware (see workdir/logs):
//! - The controller does not send a reply identical to its previous reply, so the same command twice
//!   in a row gets no answer. A different command has to be sent in between.
//! - Reading or writing a slot's config makes that slot active.
//! - Slot switch replies are unreliable; the active slot has to be polled.
//! - After writes and switches the controller can be unresponsive for a while.

use std::time::{Duration, Instant};

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

/// Per-slot data blobs, transferred in 10-byte packets.
#[derive(Debug, Clone, Copy)]
pub enum Blob {
    /// Button mapping, sticks, triggers, rumble, motion, macros (840 bytes).
    Mapping,
    /// LED colours and effects (500 bytes).
    Led,
}

impl Blob {
    fn read_cmd(self) -> u8 {
        match self {
            Blob::Mapping => 0x21,
            Blob::Led => 0x26,
        }
    }
    fn read_reply(self) -> u8 {
        self.read_cmd() + 1
    }
    /// "Write all" start command; the official app uses it to persist configs of old-protocol devices.
    fn write_all_cmd(self) -> u8 {
        match self {
            Blob::Mapping => 0x25,
            Blob::Led => 0x2a,
        }
    }
    fn packet_cmd(self) -> u8 {
        match self {
            Blob::Mapping => 0x24,
            Blob::Led => 0x29,
        }
    }
    /// Number of packets, from the first packet's payload.
    fn packet_count(self, first: &[u8]) -> usize {
        match (self, first[1], first[0]) {
            (Blob::Mapping, 3, 0) => 79,
            (Blob::Mapping, 3, 1) => 84,
            (Blob::Mapping, _, _) => first[2] as usize,
            (Blob::Led, 2 | 3, 0) => 50,
            (Blob::Led, _, _) => 0,
        }
    }
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
        // The last reply of a previous session is unknown; after these two it is a known heartbeat.
        c.separator(0x20)?;
        c.separator(0x10)?;
        Ok(c)
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

    /// Writes `req`, first sending a different command if the reply would repeat the previous one.
    fn send(&self, req: &[u8]) -> Result<()> {
        let last = self.t.last_sent();
        let same_cmd = last.get(1) == req.get(1)
            && (req[1] != 0x50 || last.get(2) == req.get(2))
            // Replies to these echo the slot or the data, so they differ anyway.
            && !matches!(req[1], 0x21 | 0x24 | 0x26 | 0x29);
        if same_cmd || self.t.would_repeat(req) {
            self.separator(if req[1] == 0x10 { 0x20 } else { 0x10 })?;
        }
        self.t.write(req)
    }

    /// Writes `req` without waiting for a reply (for exploration).
    pub fn send_raw(&self, req: &[u8]) -> Result<()> {
        self.send(req)
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

    /// Waits until the controller answers again.
    fn wait_responsive(&self, max: Duration) -> Result<()> {
        let start = Instant::now();
        while start.elapsed() < max {
            if self.current_config_id().is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        bail!("controller unresponsive for {max:?}")
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

    /// Returns (active slot, active Switch-mode slot if reported).
    pub fn current_config_id(&self) -> Result<(u8, Option<u8>)> {
        let d = self.request(&cmd(0x20, &[]), |p| p[15] == 0x20)?;
        Ok((d[16], (d[17] == 0xa0).then_some(d[18])))
    }

    /// The data_version field (offset 225) of each slot's mapping config.
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
        // The switch reply ([17] = 1) sometimes arrives late or not at all although the switch
        // happened, so success is judged by polling the active slot.
        let switch = cmd(0x50, &[5, slot]);
        let start = Instant::now();
        let mut resent = 0u64;
        self.send(&switch)?;
        while start.elapsed() < Duration::from_secs(10) {
            if resent < 3 && start.elapsed() > Duration::from_millis(1500 * (resent + 1)) {
                self.send(&switch)?;
                resent += 1;
            }
            self.send(&cmd(0x20, &[]))?;
            if let Some(d) = self.t.read_until(Duration::from_millis(200), |p| is_reply(p) && p[15] == 0x20)? {
                if d[16] == slot {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        bail!("slot {slot} did not become active within 10 s")
    }

    /// Reads a blob without restoring the active slot (reading makes `slot` active).
    pub fn read_blob_raw(&self, blob: Blob, slot: u8) -> Result<Vec<u8>> {
        'retry: for _ in 0..RETRIES {
            self.send(&cmd(blob.read_cmd(), &[slot]))?;
            let mut count: Option<usize> = None;
            let mut data: Vec<u8> = Vec::new();
            let mut received: Vec<bool> = Vec::new();
            loop {
                let Some(p) = self.t.read_until(TIMEOUT, |p| is_reply(p) && p[15] == blob.read_reply())? else {
                    continue 'retry;
                };
                let idx = p[16] as usize;
                if count.is_none() {
                    if idx != 0 {
                        continue;
                    }
                    let n = blob.packet_count(&p[17..27]);
                    if n == 0 {
                        bail!("unsupported {blob:?} data version {:02x} {:02x}", p[17], p[18]);
                    }
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
                    continue 'retry;
                }
            }
        }
        bail!("could not read {blob:?} config of slot {slot}")
    }

    fn read_blob_retrying(&self, blob: Blob, slot: u8) -> Result<Vec<u8>> {
        for _ in 0..2 {
            if let Ok(d) = self.read_blob_raw(blob, slot) {
                return Ok(d);
            }
            self.wait_responsive(Duration::from_secs(5))?;
        }
        self.read_blob_raw(blob, slot)
    }

    /// Writes a complete blob with the "write all" command, which persists it.
    pub fn write_blob_all(&self, blob: Blob, slot: u8, data: &[u8]) -> Result<()> {
        if data.len() % 10 != 0 {
            bail!("blob length must be a multiple of 10");
        }
        let packets: Vec<&[u8]> = data.chunks(10).collect();
        // Packets must not be resent individually: the controller counts every packet it receives
        // (ack [16]) and silently drops the whole transfer if the count ends up wrong. So any lost or
        // unexpected ack restarts the transfer with a new start command.
        for attempt in 1..=RETRIES {
            match self.try_write_blob_all(blob, slot, &packets) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if self.t.trace {
                        eprintln!("write attempt {attempt} failed: {e}");
                    }
                    self.wait_responsive(Duration::from_secs(5))?;
                }
            }
        }
        bail!("writing {blob:?} config of slot {slot} failed {RETRIES} times")
    }

    fn try_write_blob_all(&self, blob: Blob, slot: u8, packets: &[&[u8]]) -> Result<()> {
        let start = blob.write_all_cmd();
        self.request(&cmd(start, &[packets.len() as u8, 0xa0, slot]), |p| p[15] == start)?;
        for (i, chunk) in packets.iter().enumerate() {
            let mut args = [0u8; 12];
            args[..10].copy_from_slice(chunk);
            args[10] = 0xa0;
            args[11] = i as u8;
            self.send(&cmd(blob.packet_cmd(), &args))?;
            // The first packet can take long to be acknowledged (flash erase?).
            let timeout = if i == 0 { Duration::from_secs(3) } else { Duration::from_secs(1) };
            let Some(ack) = self.t.read_until(timeout, |p| is_reply(p) && p[15] == blob.packet_cmd())? else {
                bail!("no ack for packet {i}");
            };
            if ack[16] as usize != i + 1 || &ack[17..27] != *chunk {
                bail!("unexpected ack for packet {i}: count {}", ack[16]);
            }
        }
        Ok(())
    }

    /// Reads a slot's blob and restores the previously active slot.
    pub fn read_blob(&self, blob: Blob, slot: u8) -> Result<Vec<u8>> {
        let active = self.current_config_id()?.0;
        let data = self.read_blob_retrying(blob, slot);
        self.apply_config(active)?;
        data
    }

    /// Persistently writes a complete blob to `slot`, verifies it by reading it back and restores
    /// the previously active slot.
    pub fn write_blob(&self, blob: Blob, slot: u8, data: &[u8]) -> Result<()> {
        let active = self.current_config_id()?.0;
        self.write_blob_all(blob, slot, data)?;
        self.wait_responsive(Duration::from_secs(10))?;
        let back = self.read_blob_retrying(blob, slot);
        self.apply_config(active)?;
        if back? != data {
            bail!("read-back of slot {slot} differs from what was written");
        }
        Ok(())
    }

    pub fn read_config(&self, slot: u8) -> Result<Vec<u8>> {
        self.read_blob(Blob::Mapping, slot)
    }

    pub fn write_config(&self, slot: u8, data: &[u8]) -> Result<()> {
        if data.len() != crate::config::CONFIG_LEN {
            bail!("config must be {} bytes", crate::config::CONFIG_LEN);
        }
        self.write_blob(Blob::Mapping, slot, data)
    }
}
