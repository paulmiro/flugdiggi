//! Packet layout of the old (`XInput`) Flydigi protocol.
//!
//! Requests are 15 bytes `[0xA5, cmd, args...]` written to the XUSB OUT endpoint. Replies arrive
//! interleaved with input reports as 32-byte packets with `[14] = 0xA5|0x5A` and `[15]` = reply id.

use crate::types::{Battery, Connection, DeviceInfo, Firmware, Mac, Slot};

pub(crate) type Packet = [u8; 32];
pub(crate) type Request = [u8; 15];
/// Blobs are transferred in packets of this many bytes.
pub(crate) type Chunk = [u8; 10];

const PREFIX: u8 = 0xa5;
pub(crate) const HEARTBEAT: u8 = 0x10;
pub(crate) const ACTIVE_SLOT: u8 = 0x20;
pub(crate) const SYSTEM: u8 = 0x50;
pub(crate) const SYS_SAVE: u8 = 3;
pub(crate) const SYS_VERSIONS: u8 = 4;
pub(crate) const SYS_SWITCH: u8 = 5;
/// Byte 17 of an active-slot reply when byte 18 holds the Switch-mode slot.
const SWITCH_MARK: u8 = 0xa0;

pub(crate) fn request(id: u8, args: &[u8]) -> Request {
    let mut p = [0u8; 15];
    p[0] = PREFIX;
    p[1] = id;
    for (dst, src) in p.iter_mut().skip(2).zip(args) {
        *dst = *src;
    }
    p
}

/// Replies to commands carry a 0xA5/0x5A marker at byte 14; everything else is an input report.
pub(crate) fn is_reply(p: &Packet) -> bool {
    p[14] == 0xa5 || p[14] == 0x5a
}

pub(crate) fn is_reply_to(p: &Packet, id: u8) -> bool {
    is_reply(p) && p[15] == id
}

/// A plausible active-slot reply. Replies are sometimes torn: bytes 15-17 still hold the previous reply
/// while the rest is the new one, or vice versa. A torn reply once reported the device type (105) as the
/// active slot, and switching to it put the controller on a nonexistent config.
pub(crate) fn is_active_slot_reply(p: &Packet) -> bool {
    is_reply_to(p, ACTIVE_SLOT) && p[16] <= Slot::MAX && (p[17] != SWITCH_MARK || (4..=Slot::MAX).contains(&p[18]))
}

pub(crate) fn active_slot(p: &Packet) -> Option<(Slot, Option<Slot>)> {
    let slot = Slot::new(p[16]).ok()?;
    Some((slot, (p[17] == SWITCH_MARK).then(|| Slot::new(p[18]).ok()).flatten()))
}

/// The 10 payload bytes of a blob transfer packet.
pub(crate) fn chunk(p: &Packet) -> Chunk {
    let mut c = [0u8; 10];
    c.copy_from_slice(&p[17..27]);
    c
}

pub(crate) fn device_info(p: &Packet) -> DeviceInfo {
    let charging = p[23] & 0xf0 == 0x10;
    DeviceInfo {
        device_type: p[16],
        mac: Mac([p[20], p[19], p[18], p[17]]),
        firmware: Firmware([p[22] >> 4, p[22] & 0xf, p[21] >> 4, p[21] & 0xf]),
        battery: if charging { Battery::Charging } else { Battery::Level(p[23] & 0xf) },
        chip_type: p[24],
        connection: if p[25] == 1 { Connection::Cable } else { Connection::Dongle },
        motion_chip: p[26],
        config_id: p[27],
    }
}
