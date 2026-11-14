//! Connection handling and simple commands.
//!
//! Quirks observed on hardware (see workdir/logs):
//! - The controller does not send a reply identical to its previous reply, so the same command twice
//!   in a row gets no answer. A different command has to be sent in between.
//! - Reading or writing a slot's config makes that slot active.
//! - Slot switch replies are unreliable; the active slot has to be polled.
//! - After writes and switches the controller can be unresponsive for a while.

use std::time::{Duration, Instant};

use tracing::{debug, instrument, warn};

use crate::error::{Error, Result};
use crate::protocol::{
    ACTIVE_SLOT, HEARTBEAT, Packet, Request, SYS_SWITCH, SYS_VERSIONS, SYSTEM, active_slot, device_info,
    is_active_slot_reply, is_reply, is_reply_to, request,
};
use crate::types::{ActiveSlot, DeviceInfo, DeviceSummary, Mac, RawReply, Selector, Slot, UsbPort};
use crate::usb::{Candidate, Transport, candidates};

pub(crate) const TIMEOUT: Duration = Duration::from_millis(500);
/// How long the controller may ignore commands (after switches and saves) before an operation fails.
/// Over the dongle, 20 s was sometimes not enough.
pub(crate) const UNRESPONSIVE_MAX: Duration = Duration::from_secs(60);
const PROBE_TRIES: usize = 5;
const RECONNECT_MAX: Duration = Duration::from_secs(15);
const RAW_ALLOWED: fn(&[u8]) -> bool = |c| matches!(c, [HEARTBEAT, ..] | [ACTIVE_SLOT] | [SYSTEM, SYS_VERSIONS]);

/// An opened Flydigi controller (old protocol: Vader 4 Pro via dongle or cable).
///
/// While it is open the kernel driver is detached, so games don't see the controller. It is released
/// on drop. All operations block; long ones (writes) take several seconds.
#[derive(Debug)]
pub struct Controller {
    pub(crate) t: Transport,
    /// MAC of the selected controller; a reconnect only accepts a device that reports it.
    mac: Option<Mac>,
}

impl Controller {
    /// Opens the controller chosen by `selector`.
    #[instrument(level = "debug")]
    pub fn open(selector: &Selector) -> Result<Self> {
        let cands = candidates()?;
        if cands.is_empty() {
            return Err(Error::NoDevice);
        }
        let chosen = match (selector, cands.as_slice()) {
            (Selector::Port(port), _) => {
                Some(cands.iter().find(|c| c.port == *port).ok_or_else(|| Error::NoDeviceOnPort(port.clone()))?)
            }
            (Selector::Only, [only]) => Some(only),
            (Selector::Only, _) => {
                return Err(Error::SeveralDevices { ports: cands.into_iter().map(|c| c.port).collect() });
            }
            (Selector::Mac(_), _) => None,
        };
        if let Some(cand) = chosen {
            let mut c = Self::connect(cand)?;
            c.mac = c.probe().map(|i| i.mac);
            return Ok(c);
        }
        let mut answering = Vec::new();
        for cand in &cands {
            let mut c = match Self::connect(cand) {
                Ok(c) => c,
                Err(e) => {
                    warn!(port = %cand.port, "skipping device: {e}");
                    continue;
                }
            };
            let Some(info) = c.probe() else { continue };
            if *selector == Selector::Mac(info.mac) {
                c.mac = Some(info.mac);
                return Ok(c);
            }
            answering.push(info.mac);
        }
        match selector {
            Selector::Mac(mac) => Err(Error::NoDeviceWithMac { mac: *mac, answering }),
            _ => Err(Error::Internal("unhandled selector")),
        }
    }

    /// Lists all Flydigi USB devices, with their heartbeat if `probe` is set.
    ///
    /// Without `probe` no device is claimed. With it, every controller briefly disconnects from its
    /// kernel driver (games see them disconnect and reconnect), because the heartbeat can only be read by
    /// claiming the device.
    pub fn list(probe: bool) -> Result<Vec<DeviceSummary>> {
        Ok(candidates()?
            .into_iter()
            .map(|cand| {
                let info = if probe {
                    match Self::connect(&cand) {
                        Ok(mut c) => c.probe(),
                        Err(e) => {
                            warn!(port = %cand.port, "cannot probe device: {e}");
                            None
                        }
                    }
                } else {
                    None
                };
                let guessed = if cand.wired_guess { crate::Connection::Cable } else { crate::Connection::Dongle };
                DeviceSummary {
                    connection: info.as_ref().map_or(guessed, |i| i.connection),
                    connection_guessed: info.is_none(),
                    port: cand.port,
                    product: cand.product,
                    info,
                }
            })
            .collect())
    }

    fn connect(cand: &Candidate) -> Result<Self> {
        let mut c = Controller { t: Transport::open(cand)?, mac: None };
        // The last reply of a previous session is unknown; after these two it is a known heartbeat.
        c.separator(ACTIVE_SLOT)?;
        c.separator(HEARTBEAT)?;
        Ok(c)
    }

    pub fn port(&self) -> &UsbPort {
        self.t.port()
    }

    /// The USB product string.
    pub fn product(&self) -> &str {
        self.t.product()
    }

    /// The MAC, if the controller answered a heartbeat when it was opened.
    pub fn mac(&self) -> Option<Mac> {
        self.mac
    }

    /// A heartbeat with a short timeout, for telling devices apart without waiting for a sleeping one.
    fn probe(&mut self) -> Option<DeviceInfo> {
        for _ in 0..PROBE_TRIES {
            let req = request(HEARTBEAT, &[self.nonce()]);
            if let Err(e) = self.send(&req) {
                debug!("heartbeat probe failed: {e}");
                return None;
            }
            if let Ok(Some(p)) = self.t.read_until(Duration::from_millis(200), |p| is_reply_to(p, HEARTBEAT)) {
                return Some(device_info(&p));
            }
        }
        debug!(port = %self.port(), "no answer to {PROBE_TRIES} heartbeat probes");
        None
    }

    pub(crate) fn separator(&mut self, id: u8) -> Result<()> {
        let req = if id == HEARTBEAT { request(HEARTBEAT, &[self.nonce()]) } else { request(id, &[]) };
        self.t.write(&req)?;
        self.t.read_until(Duration::from_millis(150), |p| is_reply_to(p, id))?;
        Ok(())
    }

    /// A heartbeat argument that differs from the last one sent.
    pub(crate) fn nonce(&self) -> u8 {
        let last = self.t.last_sent().filter(|l| l[1] == HEARTBEAT).map(|l| l[2]);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x5c, |d| d.subsec_nanos().to_le_bytes()[0]);
        if Some(now) == last { now.wrapping_add(1) } else { now }
    }

    /// Writes `req`, first sending a different command if the reply would repeat the previous one.
    pub(crate) fn send(&mut self, req: &Request) -> Result<()> {
        if let Some(last) = self.t.last_sent() {
            let same_cmd = last[1] == req[1]
                && (req[1] != SYSTEM || last[2] == req[2])
                // Replies to these echo the slot or the data, so they differ anyway.
                && !matches!(req[1], 0x21 | 0x24 | 0x26 | 0x29);
            if same_cmd || last == req {
                self.separator(if req[1] == HEARTBEAT { ACTIVE_SLOT } else { HEARTBEAT })?;
            }
        }
        self.t.write(req)
    }

    /// Sends `req` and returns the first reply matching `ack`, retrying until [`UNRESPONSIVE_MAX`].
    pub(crate) fn request(&mut self, req: &Request, ack: impl Fn(&Packet) -> bool) -> Result<Packet> {
        self.request_timeout(req, TIMEOUT, ack)
    }

    pub(crate) fn request_timeout(
        &mut self,
        req: &Request,
        timeout: Duration,
        ack: impl Fn(&Packet) -> bool,
    ) -> Result<Packet> {
        let start = Instant::now();
        let mut attempts = 0u32;
        while start.elapsed() < UNRESPONSIVE_MAX {
            let r = self.send(req).and_then(|()| self.t.read_until(timeout, |p| is_reply(p) && ack(p)));
            match r {
                Ok(Some(p)) => {
                    if attempts > 0 {
                        debug!("command {:#04x} answered after {attempts} retries", req[1]);
                    }
                    return Ok(p);
                }
                Ok(None) => {
                    self.reconnect_if_gone()?;
                }
                Err(e) => self.recover_usb_error(e)?,
            }
            attempts = attempts.saturating_add(1);
        }
        Err(Error::NoReply { command: req[1], waited: UNRESPONSIVE_MAX })
    }

    /// Reopens the device if it has re-enumerated. Some seconds after a save the controller sometimes
    /// drops its link (it goes silent, ~2 s later the dongle re-enumerates). Returns true if reopened.
    pub(crate) fn reconnect_if_gone(&mut self) -> Result<bool> {
        if !self.t.is_gone() {
            return Ok(false);
        }
        debug!(port = %self.port(), "device disappeared, waiting for it to come back");
        let start = Instant::now();
        loop {
            if let Some(c) = self.find_again() {
                self.t = c.t;
                warn!(port = %self.port(), "the controller re-enumerated; reconnected");
                return Ok(true);
            }
            if start.elapsed() > RECONNECT_MAX {
                return Err(Error::Disconnected { port: self.port().clone(), waited: RECONNECT_MAX });
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// The re-enumerated device: the one back on our USB port, if it reports our MAC (when known).
    /// Devices on other ports are not opened.
    fn find_again(&self) -> Option<Controller> {
        let cand = candidates().ok()?.into_iter().find(|c| c.port == *self.port())?;
        let mut c = Self::connect(&cand).map_err(|e| debug!("reopening failed: {e}")).ok()?;
        match self.mac {
            Some(mac) => {
                let found = c.probe()?.mac;
                if found == mac {
                    Some(c)
                } else {
                    warn!(port = %self.port(), "a controller with another MAC ({found}) appeared on our port; ignoring it");
                    None
                }
            }
            None => Some(c),
        }
    }

    /// Called after a USB error: waits briefly for the device to disappear and reopens it.
    pub(crate) fn recover_usb_error(&mut self, err: Error) -> Result<()> {
        debug!("USB error, checking whether the device re-enumerated: {err}");
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
    pub(crate) fn wait_responsive(&mut self, max: Duration) -> Result<()> {
        let start = Instant::now();
        while start.elapsed() < max {
            self.reconnect_if_gone()?;
            if self.active_slot().is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err(Error::Unresponsive(max))
    }

    /// Device type, MAC, firmware, battery and connection.
    pub fn info(&mut self) -> Result<DeviceInfo> {
        let nonce = self.nonce();
        let p = self.request(&request(HEARTBEAT, &[nonce]), |p| p[15] == HEARTBEAT)?;
        Ok(device_info(&p))
    }

    pub fn active_slot(&mut self) -> Result<ActiveSlot> {
        let p = self.request(&request(ACTIVE_SLOT, &[]), is_active_slot_reply)?;
        let (slot, switch_mode_slot) = active_slot(&p).ok_or(Error::Internal("unchecked active slot reply"))?;
        Ok(ActiveSlot { slot, switch_mode_slot })
    }

    /// The data version (offset 225 of the mapping config) of slots 0-3.
    pub fn slot_versions(&mut self) -> Result<[u16; 4]> {
        let p = self.request(&request(SYSTEM, &[SYS_VERSIONS]), |p| p[15] == SYSTEM && p[16] == SYS_VERSIONS)?;
        let v = |i: usize| crate::bytes::u16_le(&p, i);
        Ok([v(17)?, v(19)?, v(21)?, v(23)?])
    }

    /// Makes `slot` (0-3) the active config.
    #[instrument(level = "debug", skip(self))]
    pub fn set_active_slot(&mut self, slot: Slot) -> Result<()> {
        if slot.is_switch_mode() {
            return Err(Error::NotSelectable(slot));
        }
        self.switch_to(slot)
    }

    /// Makes `slot` active; unlike [`Self::set_active_slot`] also Switch-mode slots.
    pub(crate) fn switch_to(&mut self, slot: Slot) -> Result<()> {
        // The controller does not answer a switch to the slot that is already active.
        if self.active_slot()?.slot == slot {
            return Ok(());
        }
        debug!("switching to slot {slot}");
        // The switch reply sometimes arrives late or not at all although the switch happened, so success
        // is judged by polling the active slot.
        let switch = request(SYSTEM, &[SYS_SWITCH, slot.get()]);
        let start = Instant::now();
        let mut last_switch: Option<Instant> = None;
        // About a quarter of switch commands are ignored; most switches complete within 250 ms. So resend
        // soon, then less often in case the controller is unresponsive for longer.
        let mut resend_after = Duration::from_millis(400);
        while start.elapsed() < UNRESPONSIVE_MAX {
            let resend = last_switch.is_none_or(|t| t.elapsed() > resend_after);
            if resend && last_switch.is_some() {
                resend_after = resend_after.saturating_mul(2).min(Duration::from_millis(3200));
                debug!("slot switch not done yet, resending");
            }
            match self.switch_poll(resend.then_some(&switch)) {
                Ok(Some(p)) if p[16] == slot.get() => return Ok(()),
                Ok(_) => {
                    if resend {
                        last_switch = Some(Instant::now());
                    }
                }
                Err(e) => {
                    self.recover_usb_error(e)?;
                    last_switch = None;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(Error::SwitchTimeout { slot, waited: UNRESPONSIVE_MAX })
    }

    fn switch_poll(&mut self, switch: Option<&Request>) -> Result<Option<Packet>> {
        if let Some(s) = switch {
            self.send(s)?;
        }
        self.send(&request(ACTIVE_SLOT, &[]))?;
        self.t.read_until(Duration::from_millis(200), is_active_slot_reply)
    }

    /// Sends one read-only command (`[cmd, args...]`; heartbeat 0x10, active slot 0x20 or slot versions
    /// 0x50 04) and collects all replies arriving within `window`.
    pub fn query_raw(&mut self, command: &[u8], window: Duration) -> Result<(Request, Vec<RawReply>)> {
        if !RAW_ALLOWED(command) || command.len() > 13 {
            return Err(Error::NotAllowed(command.to_vec()));
        }
        let (id, args) = command.split_first().ok_or_else(|| Error::NotAllowed(Vec::new()))?;
        let req = request(*id, args);
        self.send(&req)?;
        let start = Instant::now();
        let mut replies = Vec::new();
        while let Some(packet) = self.t.read_until(window.saturating_sub(start.elapsed()), is_reply)? {
            replies.push(RawReply { after: start.elapsed(), packet });
        }
        Ok((req, replies))
    }
}
