//! Reading and writing the per-slot blobs.

use std::ops::Range;
use std::time::{Duration, Instant};

use tracing::{debug, error, instrument, warn};

use crate::config::{self, Config};
use crate::controller::{Controller, TIMEOUT, UNRESPONSIVE_MAX};
use crate::error::{BoxError, Error, Result};
use crate::led::Led;
use crate::protocol::{
    Chunk, HEARTBEAT, Packet, SYS_SAVE, SYSTEM, chunk, is_active_slot_reply, is_reply, is_reply_to, request,
};
use crate::types::{Blob, Slot, Version, WriteOutcome};

const RETRIES: usize = 3;
const REPAIRS: usize = 2;
// About 1 s after a save the controller is silent for ~1-2 s (the flash write); switching slots after that
// kept the save in all tests. If no such silence shows up (e.g. over a cable), the wait falls back to
// SAVE_SETTLE. (Session 1: a fixed 3 s once let an LED save corrupt the mapping, 8 s worked.)
const SAVE_SETTLE: Duration = Duration::from_secs(8);
const SAVE_SILENCE_MIN: Duration = Duration::from_millis(500);
const SAVE_MARGIN: Duration = Duration::from_millis(500);
/// The mapping packet holding the config name.
const NAME_PACKET: usize = 77;

impl Blob {
    fn read_cmd(self) -> u8 {
        match self {
            Blob::Mapping => 0x21,
            Blob::Led => 0x26,
        }
    }

    fn read_reply(self) -> u8 {
        match self {
            Blob::Mapping => 0x22,
            Blob::Led => 0x27,
        }
    }

    fn other(self) -> Blob {
        match self {
            Blob::Mapping => Blob::Led,
            Blob::Led => Blob::Mapping,
        }
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

    /// Number of packets, from the first packet's payload. 0 = unsupported.
    fn packet_count(self, first: &Chunk) -> usize {
        match (self, first[1], first[0]) {
            (Blob::Mapping, 3, 0) => 79,
            (Blob::Mapping, 3, 1) => 84,
            (Blob::Mapping, _, _) => usize::from(first[2]),
            (Blob::Led, 2 | 3, 0) => 50,
            (Blob::Led, _, _) => 0,
        }
    }
}

fn chunks(d: &[u8]) -> Vec<Chunk> {
    d.as_chunks::<10>().0.to_vec()
}

/// Ranges of consecutive packets that differ between `old` and `new`.
fn changed_runs(old: &[Chunk], new: &[Chunk]) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    let mut start = None;
    let changed = old.iter().zip(new).map(|(a, b)| a != b).chain([false]);
    for (i, changed) in changed.enumerate() {
        match (changed, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                runs.push(s..i);
                start = None;
            }
            _ => {}
        }
    }
    runs
}

enum ReadError {
    /// The controller answered with the other blob's data (it sometimes keeps doing that for minutes).
    WrongBlob,
    Failed(Error),
}

impl From<Error> for ReadError {
    fn from(e: Error) -> Self {
        ReadError::Failed(e)
    }
}

enum WriteError {
    /// The controller ignored a write start command. Pushing such a write through (resending the start
    /// and saving) often saved a stale LED config, so the write is restarted with a fresh slot load.
    StartIgnored,
    /// The read-back after a save did not match.
    Verify(Box<VerifyFailed>),
    Failed(Error),
}

impl From<Error> for WriteError {
    fn from(e: Error) -> Self {
        WriteError::Failed(e)
    }
}

struct VerifyFailed {
    reason: String,
    /// The content both blobs should have.
    expected: [(Blob, Vec<u8>); 2],
    version: u16,
}

/// Collects the packets of a blob transfer.
struct Assembly(Vec<Option<Chunk>>);

impl Assembly {
    fn put(&mut self, idx: usize, c: Chunk) {
        if let Some(slot) = self.0.get_mut(idx) {
            *slot = Some(c);
        }
    }

    fn is_last(&self, idx: usize) -> bool {
        idx.checked_add(1) == Some(self.0.len())
    }

    fn finish(&self) -> Option<Vec<u8>> {
        self.0
            .iter()
            .map(|c| c.as_ref())
            .collect::<Option<Vec<_>>>()
            .map(|c| c.into_iter().flatten().copied().collect())
    }
}

impl Controller {
    /// Reads a slot's blob. Reading makes the slot active; the previously active slot is restored.
    #[instrument(level = "debug", skip(self))]
    pub fn read(&mut self, blob: Blob, slot: Slot) -> Result<Vec<u8>> {
        let active = self.active_slot()?.slot;
        let data = self.read_retrying(blob, slot);
        self.switch_to(active)?;
        data
    }

    /// Reads the mapping and LED config of every slot in `slots`, in order.
    ///
    /// Reading makes a slot active, and switching back is slow (the controller ignores about a quarter of
    /// the switch commands), so unlike [`Self::read`] this restores the previously active slot once, at the
    /// end. `progress` is called with the number of finished slots. On an error the active slot is restored
    /// too.
    #[instrument(level = "debug", skip(self, progress))]
    pub fn read_slots(&mut self, slots: &[Slot], mut progress: impl FnMut(usize)) -> Result<Vec<(Config, Led)>> {
        let active = self.active_slot()?.slot;
        let mut blobs = Vec::with_capacity(slots.len());
        let mut failure = None;
        for &slot in slots {
            let pair =
                self.read_retrying(Blob::Mapping, slot).and_then(|m| Ok((m, self.read_retrying(Blob::Led, slot)?)));
            match pair {
                Ok(pair) => {
                    blobs.push(pair);
                    progress(blobs.len());
                }
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        let restored = self.switch_to(active);
        if let Some(e) = failure {
            if let Err(r) = restored {
                warn!("could not restore slot {active} after a failed read: {r}");
            }
            return Err(e);
        }
        restored?;
        blobs.iter().map(|(m, l)| Ok((Config::decode(m)?, Led::decode(l)?))).collect()
    }

    pub fn read_config(&mut self, slot: Slot) -> Result<Config> {
        Config::decode(&self.read(Blob::Mapping, slot)?)
    }

    pub fn read_led(&mut self, slot: Slot) -> Result<Led> {
        Led::decode(&self.read(Blob::Led, slot)?)
    }

    /// Reads a blob without restoring the active slot.
    fn read_once(&mut self, blob: Blob, slot: Slot) -> Result<Vec<u8>, ReadError> {
        let wrong = blob.other().read_reply();
        let mut wrong_seen = false;
        'retry: for attempt in 0..RETRIES {
            if attempt > 0 {
                debug!("{blob} read of slot {slot} incomplete, retrying");
            }
            if let Err(e) = self.send(&request(blob.read_cmd(), &[slot.get()])) {
                self.recover_usb_error(e)?;
                continue;
            }
            let mut asm: Option<Assembly> = None;
            loop {
                let wanted = |p: &Packet| is_reply(p) && (p[15] == blob.read_reply() || p[15] == wrong);
                let p = match self.t.read_until(TIMEOUT, wanted) {
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
                let idx = usize::from(p[16]);
                let c = chunk(&p);
                let asm = match &mut asm {
                    Some(a) => a,
                    None if idx != 0 => continue,
                    None => match blob.packet_count(&c) {
                        0 => {
                            return Err(Error::UnsupportedBlob { blob, len: 0, version: [c[0], c[1]] }.into());
                        }
                        n => asm.insert(Assembly(vec![None; n])),
                    },
                };
                asm.put(idx, c);
                if asm.is_last(idx) {
                    match asm.finish() {
                        Some(d) => return Ok(d),
                        None => continue 'retry,
                    }
                }
            }
        }
        if wrong_seen { Err(ReadError::WrongBlob) } else { Err(Error::ReadFailed { blob, slot }.into()) }
    }

    /// After slot switches and saves the controller sometimes ignores (read) commands for many seconds.
    fn read_retrying(&mut self, blob: Blob, slot: Slot) -> Result<Vec<u8>> {
        let start = Instant::now();
        let mut read_other_first = true;
        loop {
            match self.read_once(blob, slot) {
                Ok(d) => return Ok(d),
                Err(ReadError::Failed(e)) if start.elapsed() > UNRESPONSIVE_MAX => return Err(e),
                Err(ReadError::WrongBlob) if start.elapsed() > UNRESPONSIVE_MAX => {
                    return Err(Error::ReadFailed { blob, slot });
                }
                Err(ReadError::WrongBlob) => {
                    if read_other_first {
                        warn!("{blob} read of slot {slot} got {} data; reading that first", blob.other());
                        if self.read_once(blob.other(), slot).is_err() {
                            debug!("reading the {} of slot {slot} failed too", blob.other());
                        }
                    } else {
                        warn!("{blob} read of slot {slot} got {} data; switching slots", blob.other());
                        self.switch_to(slot.away())?;
                        self.switch_to(slot)?;
                    }
                    read_other_first = !read_other_first;
                    continue;
                }
                Err(ReadError::Failed(e)) => debug!("{blob} read of slot {slot} failed: {e}"),
            }
            self.wait_responsive(UNRESPONSIVE_MAX)?;
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// Persistently writes a slot's blob the way the official app does: the changed packets are written
    /// to the slot's working copy (which makes the slot active), the working copy is saved with the data
    /// version given by `version`, and both of the slot's blobs are read back and verified. If the
    /// verification fails, the slot is repaired. Afterwards the previously active slot is restored.
    ///
    /// `make` computes the new content from the current one (read once, after loading the slot). It
    /// must keep the length. If it returns the current content, nothing is written.
    #[instrument(level = "debug", skip(self, make))]
    pub fn write(
        &mut self,
        blob: Blob,
        slot: Slot,
        version: Version,
        make: impl FnOnce(&[u8]) -> Result<Vec<u8>, BoxError>,
    ) -> Result<WriteOutcome> {
        let before = self.active_slot()?;
        // Retries reload the slot, so `make` is called once and its result reused.
        let mut make = Some(make);
        let mut data: Option<Vec<u8>> = None;
        let mut get = |old: &[u8]| -> Result<Vec<u8>> {
            if let Some(make) = make.take() {
                let new = make(old).map_err(|e| Error::Prepare(blob, e))?;
                if new.len() != old.len() {
                    return Err(Error::WrongLength { blob, expected: old.len(), got: new.len() });
                }
                data = Some(new);
            }
            data.clone().ok_or(Error::Internal("content callback failed earlier"))
        };
        let mut result = self.write_once(blob, slot, &mut get, version, true);
        for _ in 1..RETRIES {
            if !matches!(result, Err(WriteError::StartIgnored)) {
                break;
            }
            warn!("the controller ignored the write; reloading slot {slot} and writing again");
            result = self.write_once(blob, slot, &mut get, version, true);
        }
        for _ in 0..REPAIRS {
            let Err(WriteError::Verify(failed)) = result else { break };
            warn!("verification failed ({}); repairing slot {slot}", failed.reason);
            result = self.repair(slot, &failed.expected, failed.version).map(|()| {
                let main = failed.expected.iter().find(|(b, _)| *b == blob);
                main.map_or(WriteOutcome::Unchanged, |(_, d)| WriteOutcome::Written(d.clone()))
            });
        }
        // Switching to a Switch-mode slot (4-7) also makes it the Switch-mode slot; switching to the
        // previous one first restores that.
        if let Some(s) = before.switch_mode_slot.filter(|&s| s != slot && slot.is_switch_mode()) {
            self.switch_to(s)?;
        }
        self.switch_to(before.slot)?;
        match result {
            Ok(o) => Ok(o),
            Err(WriteError::Failed(e)) => Err(e),
            Err(WriteError::StartIgnored) => Err(Error::WriteIgnored(slot)),
            Err(WriteError::Verify(f)) => Err(Error::VerifyFailed { slot, reason: f.reason }),
        }
    }

    /// Writes both blobs of `slot` to `expected` where they differ (after a failed verification).
    fn repair(&mut self, slot: Slot, expected: &[(Blob, Vec<u8>); 2], version: u16) -> Result<(), WriteError> {
        for (blob, data) in expected {
            self.write_once(*blob, slot, &mut |_: &[u8]| Ok(data.clone()), Version::Exact(version), true)?;
        }
        Ok(())
    }

    /// Discards replies until none has arrived for `quiet` (at most `max`). Returns the number discarded.
    fn drain_quiet(&self, quiet: Duration, max: Duration) -> usize {
        let start = Instant::now();
        let mut n = 0usize;
        while start.elapsed() < max {
            match self.t.read_until(quiet, is_reply) {
                Ok(Some(_)) => n = n.saturating_add(1),
                _ => break,
            }
        }
        n
    }

    fn write_once(
        &mut self,
        blob: Blob,
        slot: Slot,
        make: &mut dyn FnMut(&[u8]) -> Result<Vec<u8>>,
        version: Version,
        retry: bool,
    ) -> Result<WriteOutcome, WriteError> {
        // The save stores the controller's whole working copy (mapping + LED) into the active slot, and a
        // partial write to a slot does not load that slot into the working copy. So the slot is loaded with a
        // real switch first (not needed if it is already active: `Edit` never leaves unsaved edits), and
        // afterwards both blobs are verified from flash.
        let other_blob = blob.other();
        let mut edit = self.edit(slot)?;
        let other_before = edit.c.read_retrying(other_blob, slot)?;
        let old = edit.c.read_retrying(blob, slot)?;
        let mut new = make(&old)?;
        if old == new {
            return Ok(WriteOutcome::Unchanged);
        }
        let mapping = if blob == Blob::Mapping { &old } else { &other_before };
        let current = config::data_version(mapping)?;
        let version = match version {
            Version::New => new_version(current),
            Version::Keep => current,
            Version::Exact(v) => v,
        };
        if blob == Blob::Mapping {
            config::set_data_version(&mut new, version)?;
        }
        debug!("writing {blob} of slot {slot} with data version {version:04x}");
        let (old_p, new_p) = (chunks(&old), chunks(&new));
        for run in changed_runs(&old_p, &new_p) {
            let start = run.start;
            let packets = new_p.get(run).ok_or(Error::Internal("packet run out of range"))?;
            edit.write(blob, start, packets)?;
        }
        if blob == Blob::Led {
            // Rewrite one unchanged mapping packet (the name) so the mapping working copy is touched before
            // the save; an LED-only save once stored another slot's mapping into this slot.
            let name = chunks(&other_before).get(NAME_PACKET).copied().ok_or(Error::Internal("short mapping"))?;
            edit.write(Blob::Mapping, NAME_PACKET, &[name])?;
        }
        edit.save(version)?;
        self.wait_save_committed()?;
        // Reload from flash and verify both blobs. Reading a slot right after switching away from it can
        // return the newly active slot's data, so the slot is switched to explicitly before reading.
        self.switch_to(slot.away())?;
        self.switch_to(slot)?;
        let mut other_expected = other_before;
        if other_blob == Blob::Mapping {
            config::set_data_version(&mut other_expected, version)?;
        }
        let back = self.read_retrying(blob, slot)?;
        if back != new {
            // Removing a macro and its button's "macro" target in one write leaves the target set (the
            // firmware re-marks it); a second write of only the button table sticks.
            let only_keys =
                back.iter().zip(&new).enumerate().filter(|(_, (a, b))| a != b).all(|(i, _)| config::KEYS.contains(&i));
            if retry && blob == Blob::Mapping && only_keys {
                warn!("button table of slot {slot} differs after writing; writing it again");
                return self.write_once(blob, slot, make, Version::Exact(version), false);
            }
            let reason = format!("read-back of slot {slot} differs from what was written");
            return Err(verify_failed(reason, [(other_blob, other_expected), (blob, new)], version));
        }
        let other_after = self.read_retrying(other_blob, slot)?;
        if other_after != other_expected {
            let diff: Vec<String> = other_after
                .iter()
                .zip(&other_expected)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| format!("{i}: {a:02x} (expected {b:02x})"))
                .collect();
            let reason = format!("the {other_blob} of slot {slot} changed unexpectedly while saving: {diff:?}");
            return Err(verify_failed(reason, [(other_blob, other_expected), (blob, new)], version));
        }
        Ok(WriteOutcome::Written(new))
    }

    /// Loads `slot` into the working copy (by making it active) for editing.
    fn edit(&mut self, slot: Slot) -> Result<Edit<'_>> {
        self.switch_to(slot)?;
        Ok(Edit { c: self, slot, dirty: false })
    }

    /// Partial write of one run of packets. A failed transfer is restarted from its start command,
    /// because the controller counts received packets and drops transfers with a wrong count.
    fn write_run(&mut self, blob: Blob, slot: Slot, start: usize, packets: &[Chunk]) -> Result<(), WriteError> {
        let mut last_err = Error::Internal("no write attempt");
        for _ in 0..RETRIES {
            match self.write_partial(blob, slot, start, packets) {
                Ok(()) => return Ok(()),
                Err(WriteError::Failed(e)) => {
                    debug!("partial write failed, restarting it: {e}");
                    last_err = e;
                    self.wait_responsive(UNRESPONSIVE_MAX)?;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err.into())
    }

    /// Partially writes consecutive packets starting at packet `start` (live edit, not persisted by itself).
    fn write_partial(&mut self, blob: Blob, slot: Slot, start: usize, packets: &[Chunk]) -> Result<(), WriteError> {
        let failed = |packet: usize, reason: String| Error::WriteFailed { slot, packet, reason };
        let s = blob.partial_cmd();
        let first = u8::try_from(start).map_err(|_| failed(start, "packet index too large".into()))?;
        let count = u8::try_from(packets.len()).map_err(|_| failed(start, "too many packets".into()))?;
        self.separator(HEARTBEAT)?;
        self.send(&request(s, &[first, count, 0xa0, slot.get()]))?;
        if self.t.read_until(Duration::from_secs(1), |p| is_reply_to(p, s))?.is_none() {
            return Err(WriteError::StartIgnored);
        }
        let pkt = blob.packet_cmd();
        for (i, c) in packets.iter().enumerate() {
            let n = u8::try_from(i).map_err(|_| failed(i, "too many packets".into()))?;
            let mut args = [0u8; 12];
            args[..10].copy_from_slice(c);
            args[10] = 0xa0;
            args[11] = n;
            self.send(&request(pkt, &args))?;
            let timeout = if i == 0 { Duration::from_secs(3) } else { Duration::from_secs(1) };
            // Over the cable, replies are sometimes sent twice, so a duplicate of the previous ack is skipped.
            let Some(ack) = self.t.read_until(timeout, |p| is_reply_to(p, pkt) && p[16] > n)? else {
                return Err(failed(i, "no acknowledgement".into()).into());
            };
            if Some(ack[16]) != n.checked_add(1) || chunk(&ack) != *c {
                return Err(failed(i, format!("unexpected acknowledgement (count {})", ack[16])).into());
            }
        }
        Ok(())
    }

    /// Waits until the controller has been silent after a save (flash write) and answers again.
    fn wait_save_committed(&mut self) -> Result<()> {
        let start = Instant::now();
        let mut silent_since: Option<Instant> = None;
        let mut silence_seen = false;
        let mut heartbeat = true;
        while start.elapsed() < SAVE_SETTLE {
            let answered = self.answers_once(heartbeat);
            heartbeat = !heartbeat;
            if answered {
                if silence_seen {
                    debug!("save committed after {:?}", start.elapsed());
                    break;
                }
                silent_since = None;
            } else if silent_since.get_or_insert_with(Instant::now).elapsed() >= SAVE_SILENCE_MIN {
                silence_seen = true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if !silence_seen {
            debug!("no silence after the save; waited the full {SAVE_SETTLE:?}");
        }
        std::thread::sleep(SAVE_MARGIN);
        self.wait_responsive(UNRESPONSIVE_MAX)
    }

    /// Sends one heartbeat or active-slot request and reports whether it was answered within 100 ms.
    fn answers_once(&mut self, heartbeat: bool) -> bool {
        let req =
            if heartbeat { request(HEARTBEAT, &[self.nonce()]) } else { request(crate::protocol::ACTIVE_SLOT, &[]) };
        let ok = |p: &Packet| if heartbeat { is_reply_to(p, HEARTBEAT) } else { is_active_slot_reply(p) };
        self.send(&req).is_ok() && matches!(self.t.read_until(Duration::from_millis(100), ok), Ok(Some(_)))
    }

    /// "Permanent save" of the active slot's working copy with data version `version`.
    fn save_active(&mut self, version: u16) -> Result<()> {
        let [lo, hi] = version.to_le_bytes();
        self.request_timeout(&request(SYSTEM, &[SYS_SAVE, lo, hi]), Duration::from_secs(10), |p| {
            p[15] == SYSTEM && p[16] == SYS_SAVE
        })?;
        Ok(())
    }
}

fn verify_failed(reason: String, expected: [(Blob, Vec<u8>); 2], version: u16) -> WriteError {
    WriteError::Verify(Box::new(VerifyFailed { reason, expected, version }))
}

/// A random data version different from `current` (the official app does the same on every save).
fn new_version(current: u16) -> u16 {
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| u64::from(d.subsec_nanos()) ^ d.as_secs());
    loop {
        seed ^= seed.wrapping_shl(13);
        seed ^= seed.wrapping_shr(7);
        seed ^= seed.wrapping_shl(17);
        let [a, b, ..] = seed.to_le_bytes();
        let v = u16::from_le_bytes([a, b]);
        if v != current {
            return v;
        }
    }
}

/// Unsaved edits of a slot's working copy. Partial writes are only possible through this, and it ends either
/// with `save` or, when dropped without a successful save, by discarding the edits (switching away and back).
/// So no other operation, in particular a later write that skips loading the already active slot, can see
/// or save leftover edits.
#[must_use = "dropping an Edit discards its writes"]
struct Edit<'a> {
    c: &'a mut Controller,
    slot: Slot,
    dirty: bool,
}

impl Edit<'_> {
    /// Writes consecutive packets starting at packet `start` into the working copy (RAM only).
    fn write(&mut self, blob: Blob, start: usize, packets: &[Chunk]) -> Result<(), WriteError> {
        self.dirty = true;
        self.c.write_run(blob, self.slot, start, packets)
    }

    /// Saves the whole working copy (mapping and LED) into the slot with data version `version`.
    /// Returns once the save is acknowledged; the flash write happens afterwards.
    fn save(mut self, version: u16) -> Result<()> {
        // Sometimes (after saves) the controller falls into a mode with one read packet per 40 ms instead of
        // 4-14 ms, lags behind and keeps streaming old transfers. Saves made while it still streamed were lost
        // or stored an outdated LED config, so the stream is drained first.
        let stale = self.c.drain_quiet(Duration::from_millis(300), Duration::from_secs(15));
        if stale > 0 {
            warn!("discarded {stale} unrequested replies before saving");
        }
        if self.c.active_slot()?.slot != self.slot {
            return Err(Error::SlotChanged(self.slot));
        }
        debug!("saving slot {} with data version {version:04x}", self.slot);
        self.c.save_active(version)?;
        self.dirty = false;
        Ok(())
    }
}

impl Drop for Edit<'_> {
    fn drop(&mut self) {
        if self.dirty {
            debug!("discarding unsaved edits of slot {}", self.slot);
            let slot = self.slot;
            if let Err(e) = self.c.switch_to(slot.away()).and_then(|()| self.c.switch_to(slot)) {
                error!("could not discard unsaved edits of slot {slot}: {e}");
            }
        }
    }
}
