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
const MAX_SLOT: u8 = 7;
// How long the controller may ignore commands (after switches and saves) before an operation fails.
// Over the dongle, 20 s was sometimes not enough.
const UNRESPONSIVE_MAX: Duration = Duration::from_secs(60);
// About 1 s after a save the controller is silent for ~1-2 s (the flash write); switching slots after that
// kept the save in all tests. If no such silence shows up, the wait falls back to SAVE_SETTLE.
// (Session 1: a fixed 3 s once let an LED save corrupt the mapping, 8 s worked.)
const SAVE_SETTLE: Duration = Duration::from_secs(8);
const SAVE_SILENCE_MIN: Duration = Duration::from_millis(500);
const SAVE_MARGIN: Duration = Duration::from_millis(500);

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
    fn other(self) -> Blob {
        match self {
            Blob::Mapping => Blob::Led,
            Blob::Led => Blob::Mapping,
        }
    }
    fn read_reply(self) -> u8 {
        self.read_cmd() + 1
    }
    /// Partial write start command (live edit of the slot's working copy).
    fn partial_cmd(self) -> u8 {
        match self {
            Blob::Mapping => 0x23,
            Blob::Led => 0x28,
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

/// The controller answered a read with the other blob's data (it sometimes keeps doing that for minutes).
#[derive(Debug)]
struct WrongReply;

impl std::fmt::Display for WrongReply {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("the controller answered with the wrong config type")
    }
}

impl std::error::Error for WrongReply {}

/// The controller ignored a write start command. Pushing such a write through (resending the start
/// and saving) often saved a stale LED config, so the write is restarted with a fresh slot load.
#[derive(Debug)]
struct StartIgnored;

impl std::fmt::Display for StartIgnored {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("the controller ignored the write start command")
    }
}

impl std::error::Error for StartIgnored {}

/// The data version a write saves with.
#[derive(Debug, Clone, Copy)]
pub enum Version {
    /// A new random version (what the official app does on every save).
    New,
    /// The slot's current version.
    Keep,
    Exact(u16),
}

/// A plausible active-slot (0x20) reply. Replies are sometimes torn: bytes 15-17 still hold the previous
/// reply while the rest is the new one, or vice versa. A torn reply once reported the device type (105)
/// as the active slot, and switching to it put the controller on a nonexistent config.
fn is_config_id_reply(p: &Packet) -> bool {
    is_reply(p) && p[15] == 0x20 && p[16] <= MAX_SLOT && (p[17] != 0xa0 || (4..=MAX_SLOT).contains(&p[18]))
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
        let start = Instant::now();
        while start.elapsed() < UNRESPONSIVE_MAX {
            let r = self.send(req).and_then(|()| self.t.read_until(timeout, |p| is_reply(p) && ack(p)));
            match r {
                Ok(Some(p)) => return Ok(p),
                Ok(None) => {
                    self.reconnect_if_gone()?;
                }
                Err(e) => self.recover_usb_error(e)?,
            }
        }
        bail!("no reply to command 0x{:02x} within {UNRESPONSIVE_MAX:?}", req[1])
    }

    /// Reopens the device if it has re-enumerated. Some seconds after a save the controller sometimes
    /// drops its link (it goes silent, ~2 s later the dongle re-enumerates). Returns true if reopened.
    fn reconnect_if_gone(&self) -> Result<bool> {
        if !self.t.is_gone() {
            return Ok(false);
        }
        self.t.reconnect(Duration::from_secs(15))?;
        self.separator(0x20)?;
        self.separator(0x10)?;
        Ok(true)
    }

    /// Called after a USB error: waits briefly for the device to disappear and reopens it.
    fn recover_usb_error(&self, err: anyhow::Error) -> Result<()> {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(4) {
            if self.reconnect_if_gone()? {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err(err)
    }

    /// Waits until the controller answers again, reopening the device if it re-enumerated.
    fn wait_responsive(&self, max: Duration) -> Result<()> {
        let start = Instant::now();
        while start.elapsed() < max {
            self.reconnect_if_gone()?;
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
        let d = self.request(&cmd(0x20, &[]), is_config_id_reply)?;
        Ok((d[16], (d[17] == 0xa0).then_some(d[18])))
    }

    /// The data_version field (offset 225) of each slot's mapping config.
    pub fn config_versions(&self) -> Result<[u16; 4]> {
        let d = self.request(&cmd(0x50, &[4]), |p| p[15] == 0x50 && p[16] == 4)?;
        Ok(std::array::from_fn(|i| u16::from_le_bytes([d[17 + 2 * i], d[18 + 2 * i]])))
    }

    /// Makes `slot` the active config.
    pub fn apply_config(&self, slot: u8) -> Result<()> {
        if slot > MAX_SLOT {
            bail!("refusing to switch to nonexistent slot {slot}");
        }
        // The controller does not answer a switch to the slot that is already active.
        if self.current_config_id()?.0 == slot {
            return Ok(());
        }
        // The switch reply ([17] = 1) sometimes arrives late or not at all although the switch
        // happened, so success is judged by polling the active slot.
        let switch = cmd(0x50, &[5, slot]);
        let start = Instant::now();
        let mut last_switch: Option<Instant> = None;
        // About a quarter of switch commands are ignored; most switches complete within 250 ms. So resend
        // soon, then less often in case the controller is unresponsive for longer.
        let mut resend_after = Duration::from_millis(400);
        while start.elapsed() < UNRESPONSIVE_MAX {
            let r = (|| {
                if last_switch.is_none_or(|t| t.elapsed() > resend_after) {
                    if last_switch.is_some() {
                        resend_after = (resend_after * 2).min(Duration::from_millis(3200));
                    }
                    self.send(&switch)?;
                    last_switch = Some(Instant::now());
                }
                self.send(&cmd(0x20, &[]))?;
                self.t.read_until(Duration::from_millis(200), is_config_id_reply)
            })();
            match r {
                Ok(Some(d)) if d[16] == slot => return Ok(()),
                Ok(_) => {}
                Err(e) => {
                    self.recover_usb_error(e)?;
                    last_switch = None;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        bail!("slot {slot} did not become active within {UNRESPONSIVE_MAX:?}")
    }

    /// Reads a blob without restoring the active slot (reading makes `slot` active).
    pub fn read_blob_raw(&self, blob: Blob, slot: u8) -> Result<Vec<u8>> {
        let wrong = blob.other().read_reply();
        let mut wrong_seen = false;
        'retry: for _ in 0..RETRIES {
            if let Err(e) = self.send(&cmd(blob.read_cmd(), &[slot])) {
                self.recover_usb_error(e)?;
                continue;
            }
            let mut count: Option<usize> = None;
            let mut data: Vec<u8> = Vec::new();
            let mut received: Vec<bool> = Vec::new();
            loop {
                let p = match self.t.read_until(TIMEOUT, |p| is_reply(p) && [blob.read_reply(), wrong].contains(&p[15])) {
                    Ok(Some(p)) if p[15] == wrong => {
                        wrong_seen = true;
                        continue;
                    }
                    Ok(Some(p)) => p,
                    Ok(None) => continue 'retry,
                    Err(e) => {
                        self.recover_usb_error(e)?;
                        continue 'retry;
                    }
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
        if wrong_seen {
            return Err(WrongReply.into());
        }
        bail!("could not read {blob:?} config of slot {slot}")
    }

    /// After slot switches and saves the controller sometimes ignores (read) commands for many seconds.
    fn read_blob_retrying(&self, blob: Blob, slot: u8) -> Result<Vec<u8>> {
        let start = Instant::now();
        let mut stuck = 0;
        loop {
            match self.read_blob_raw(blob, slot) {
                Ok(d) => return Ok(d),
                Err(e) if start.elapsed() > UNRESPONSIVE_MAX => return Err(e),
                Err(e) if e.is::<WrongReply>() => {
                    stuck += 1;
                    if stuck % 2 == 1 {
                        eprintln!("{blob:?} read of slot {slot} got {:?} data; reading that first", blob.other());
                        let _ = self.read_blob_raw(blob.other(), slot);
                    } else {
                        eprintln!("{blob:?} read of slot {slot} got {:?} data; switching slots", blob.other());
                        self.apply_config(if slot == 0 { 1 } else { 0 })?;
                        self.apply_config(slot)?;
                    }
                    continue;
                }
                Err(_) => {}
            }
            self.wait_responsive(UNRESPONSIVE_MAX)?;
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// Reads a slot's blob and restores the previously active slot.
    pub fn read_blob(&self, blob: Blob, slot: u8) -> Result<Vec<u8>> {
        let active = self.current_config_id()?.0;
        let data = self.read_blob_retrying(blob, slot);
        self.apply_config(active)?;
        data
    }

    /// Persistently writes `data` to `slot` the way the official app does: the changed packets are
    /// written to the slot's working copy (which makes the slot active), the working copy is saved
    /// with a new data version and the result is read back. The previously active slot is restored.
    /// Returns the blob as stored (a mapping config's data version changes).
    /// `version` is the data version to save with; `None` picks a new random one.
    pub fn write_blob(&self, blob: Blob, slot: u8, data: &[u8], version: Option<u16>) -> Result<Vec<u8>> {
        let version = version.map_or(Version::New, Version::Exact);
        Ok(self.write_blob_with(blob, slot, |_| Ok(data.to_vec()), version)?.unwrap_or_else(|| data.to_vec()))
    }

    /// Like `write_blob`, but the new content is computed by `make` from the slot's current content, read
    /// once after loading the slot. Returns `None` if the content is unchanged (nothing is written).
    pub fn write_blob_with(
        &self,
        blob: Blob,
        slot: u8,
        make: impl FnOnce(&[u8]) -> Result<Vec<u8>>,
        version: Version,
    ) -> Result<Option<Vec<u8>>> {
        let (active, switch_slot) = self.current_config_id()?;
        // Retries reload the slot, so `make` is called once and its result reused.
        let mut make = Some(make);
        let mut data: Option<Vec<u8>> = None;
        let mut get = |old: &[u8]| -> Result<Vec<u8>> {
            if let Some(make) = make.take() {
                data = Some(make(old)?);
            }
            Ok(data.clone().expect("make failed earlier"))
        };
        let mut result = self.write_blob_inner(blob, slot, &mut get, version, true);
        for _ in 1..RETRIES {
            if !result.as_ref().is_err_and(|e| e.is::<StartIgnored>()) {
                break;
            }
            eprintln!("the controller ignored the write; reloading slot {slot} and writing again");
            result = self.write_blob_inner(blob, slot, &mut get, version, true);
        }
        // Switching to a Switch-mode slot (4-7) also makes it the Switch-mode slot; switching to the
        // previous one first restores that.
        if let Some(s) = switch_slot.filter(|&s| s != slot && slot > 3) {
            self.apply_config(s)?;
        }
        self.apply_config(active)?;
        result
    }

    fn write_blob_inner(
        &self,
        blob: Blob,
        slot: u8,
        make: &mut dyn FnMut(&[u8]) -> Result<Vec<u8>>,
        version: Version,
        retry: bool,
    ) -> Result<Option<Vec<u8>>> {
        // The save (0x50 03) stores the controller's whole working copy (mapping + LED) into the active
        // slot, and a partial write to a slot does not load that slot into the working copy. So the slot
        // is loaded with a real switch first (switching away first if it is active, which also drops
        // unsaved edits), and afterwards both blobs are verified from flash.
        let away = if slot == 0 { 1 } else { 0 };
        if self.current_config_id()?.0 == slot {
            self.apply_config(away)?;
        }
        self.apply_config(slot)?;
        let other_blob = blob.other();
        let other_before = self.read_blob_retrying(other_blob, slot)?;
        let old = self.read_blob_retrying(blob, slot)?;
        let data = make(&old)?;
        if old.len() != data.len() {
            bail!("{blob:?} data must be {} bytes", old.len());
        }
        if old == data {
            return Ok(None);
        }
        let mapping = if let Blob::Mapping = blob { &old } else { &other_before };
        let v = crate::config::DATA_VERSION;
        let current = u16::from_le_bytes([mapping[v], mapping[v + 1]]);
        let version = match version {
            Version::New => new_version(current),
            Version::Keep => current,
            Version::Exact(v) => v,
        };
        let mut new = data.to_vec();
        if let Blob::Mapping = blob {
            new[crate::config::DATA_VERSION..crate::config::DATA_VERSION + 2].copy_from_slice(&version.to_le_bytes());
        }
        let old_p: Vec<&[u8]> = old.chunks(10).collect();
        let new_p: Vec<&[u8]> = new.chunks(10).collect();
        let mut i = 0;
        while i < new_p.len() {
            if old_p[i] == new_p[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < new_p.len() && old_p[i] != new_p[i] {
                i += 1;
            }
            self.write_run(blob, slot, start, &new_p[start..i])?;
        }
        if let Blob::Led = blob {
            // Rewrite one unchanged mapping packet (the name) so the mapping working copy is touched before
            // the save; an LED-only save once stored another slot's mapping into this slot.
            let name = 77;
            self.write_run(Blob::Mapping, slot, name, &[&other_before[10 * name..10 * name + 10]])?;
        }
        if self.current_config_id()?.0 != slot {
            bail!("slot {slot} is no longer active before saving; not saving");
        }
        self.save_active(version)?;
        self.wait_save_committed()?;
        // Reload from flash and verify both blobs. Reading a slot right after switching away from it can
        // return the newly active slot's data, so the slot is switched to explicitly before reading.
        self.apply_config(away)?;
        std::thread::sleep(Duration::from_secs(1));
        self.apply_config(slot)?;
        std::thread::sleep(Duration::from_millis(500));
        let back = self.read_blob_retrying(blob, slot)?;
        if back != new {
            // Removing a macro and its button's "macro" target in one write leaves the target set (the
            // firmware re-marks it); a second write of only the button table sticks.
            let keys = crate::config::KEYS.0..crate::config::KEYS.1;
            let only_keys = (0..new.len()).filter(|&i| back[i] != new[i]).all(|i| keys.contains(&i));
            if retry && matches!(blob, Blob::Mapping) && only_keys {
                eprintln!("button table of slot {slot} differs after writing; writing it again");
                return self.write_blob_inner(blob, slot, make, Version::Exact(version), false);
            }
            bail!("read-back of slot {slot} differs from what was written");
        }
        let mut other_expected = other_before;
        if let Blob::Mapping = other_blob {
            let v = crate::config::DATA_VERSION;
            other_expected[v..v + 2].copy_from_slice(&version.to_le_bytes());
        }
        let other_after = self.read_blob_retrying(other_blob, slot)?;
        if other_after != other_expected {
            let diff: Vec<String> = (0..other_after.len())
                .filter(|&i| other_after[i] != other_expected[i])
                .map(|i| format!("{i}: {:02x} (expected {:02x})", other_after[i], other_expected[i]))
                .collect();
            bail!("the {other_blob:?} config of slot {slot} changed unexpectedly while saving: {diff:?}");
        }
        Ok(Some(new))
    }

    /// Partial write of one run of packets. A failed transfer is restarted from its start command,
    /// because the controller counts received packets and drops transfers with a wrong count.
    fn write_run(&self, blob: Blob, slot: u8, start: usize, packets: &[&[u8]]) -> Result<()> {
        let mut last_err = None;
        for _ in 0..RETRIES {
            match self.write_blob_partial(blob, slot, start as u8, packets) {
                Ok(()) => return Ok(()),
                Err(e) if e.is::<StartIgnored>() => return Err(e),
                Err(e) => {
                    last_err = Some(e);
                    self.wait_responsive(UNRESPONSIVE_MAX)?;
                }
            }
        }
        Err(last_err.unwrap())
    }

    pub fn read_config(&self, slot: u8) -> Result<Vec<u8>> {
        self.read_blob(Blob::Mapping, slot)
    }

    pub fn write_config(&self, slot: u8, data: &[u8], version: Option<u16>) -> Result<Vec<u8>> {
        if data.len() != crate::config::CONFIG_LEN {
            bail!("config must be {} bytes", crate::config::CONFIG_LEN);
        }
        self.write_blob(Blob::Mapping, slot, data, version)
    }
}

impl Controller {
    /// Partially writes consecutive packets starting at packet `start` (live edit, not persisted by itself).
    pub fn write_blob_partial(&self, blob: Blob, slot: u8, start: u8, packets: &[&[u8]]) -> Result<()> {
        let s = blob.partial_cmd();
        self.separator(0x10)?;
        self.send(&cmd(s, &[start, packets.len() as u8, 0xa0, slot]))?;
        if self.t.read_until(Duration::from_secs(1), |p| is_reply(p) && p[15] == s)?.is_none() {
            return Err(StartIgnored.into());
        }
        for (i, chunk) in packets.iter().enumerate() {
            let mut args = [0u8; 12];
            args[..10].copy_from_slice(chunk);
            args[10] = 0xa0;
            args[11] = i as u8;
            self.send(&cmd(blob.packet_cmd(), &args))?;
            let timeout = if i == 0 { Duration::from_secs(3) } else { Duration::from_secs(1) };
            // Wired, replies are sometimes sent twice, so a duplicate of the previous ack is skipped.
            let pkt = blob.packet_cmd();
            let Some(ack) = self.t.read_until(timeout, |p| is_reply(p) && p[15] == pkt && p[16] as usize > i)? else {
                bail!("no ack for packet {i}");
            };
            if ack[16] as usize != i + 1 || &ack[17..27] != *chunk {
                bail!("unexpected ack for packet {i}: count {}", ack[16]);
            }
        }
        Ok(())
    }

    /// Waits until the controller has been silent after a save (flash write) and answers again.
    fn wait_save_committed(&self) -> Result<()> {
        let start = Instant::now();
        let mut silent_since: Option<Instant> = None;
        let mut silence_seen = false;
        let mut heartbeat = true;
        while start.elapsed() < SAVE_SETTLE {
            let answered = self.answers_once(heartbeat);
            heartbeat = !heartbeat;
            if answered {
                if silence_seen {
                    break;
                }
                silent_since = None;
            } else if silent_since.get_or_insert_with(Instant::now).elapsed() >= SAVE_SILENCE_MIN {
                silence_seen = true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(SAVE_MARGIN);
        self.wait_responsive(UNRESPONSIVE_MAX)
    }

    /// Sends one heartbeat or active-slot request and reports whether it was answered within 100 ms.
    fn answers_once(&self, heartbeat: bool) -> bool {
        let (req, id) = if heartbeat { (cmd(0x10, &[self.nonce()]), 0x10) } else { (cmd(0x20, &[]), 0x20) };
        let ok = |p: &Packet| if id == 0x20 { is_config_id_reply(p) } else { is_reply(p) && p[15] == 0x10 };
        self.send(&req).is_ok() && matches!(self.t.read_until(Duration::from_millis(100), ok), Ok(Some(_)))
    }

    /// "Permanent save" (0x50 03) of the active slot's working copy with a new data version.
    pub fn save_active(&self, version: u16) -> Result<()> {
        let [lo, hi] = version.to_le_bytes();
        self.request_timeout(&cmd(0x50, &[3, lo, hi]), Duration::from_secs(10), |p| p[15] == 0x50 && p[16] == 3)?;
        Ok(())
    }
}

/// A random data version different from `current` (the official app does the same on every save).
fn new_version(current: u16) -> u16 {
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    loop {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let v = seed as u16;
        if v != current {
            return v;
        }
    }
}
