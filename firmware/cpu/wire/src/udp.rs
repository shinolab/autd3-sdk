use zerocopy::little_endian::{U16, U64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::frame::HOST_TO_DEVICE_BYTES;

pub const PORT: u16 = 0xAD30;
pub const PROTOCOL_VERSION: u8 = 1;

pub const UNASSIGNED_ID: u8 = 0xFF;
pub const MAC_PREFIX: [u8; 5] = [0x02, 0x41, 0x55, 0x54, 0x44];
pub const ALL_NODES: [u8; 16] = [0xFF, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01];
pub const ALL_NODES_MAC: [u8; 6] = [0x33, 0x33, 0x00, 0x00, 0x00, 0x01];

pub const RESET_ID_CLOSE_DELAY_MS: u32 = 20;
pub const HOST_WATCHDOG_MS: u32 = 10;
pub const SYNC_CYCLE_NS: u32 = 1_000_000;

pub const FLAG_ASSIGNED: u8 = 1 << 0;
pub const FLAG_DOWNSTREAM_OPEN: u8 = 1 << 1;
pub const FLAG_DOWNSTREAM_LINK: u8 = 1 << 2;
pub const FLAG_SYNC_READY: u8 = 1 << 3;

pub const UPSTREAM_UNKNOWN: u8 = 0xFF;

crate::wire_enum! {
    pub enum Kind {
        Frame = 0x00,
        ReadUnitInfo = 0x01,
        ResetId = 0x02,
        Discover = 0x03,
        AssignId = 0x04,
        UnblockDownstream = 0x05,
        SetTime = 0x06,
    }
}

crate::wire_enum! {
    pub enum Status {
        Ok = 0x00,
        UnsupportedVersion = 0x01,
        NotAssigned = 0x02,
        InvalidPayload = 0x03,
    }
}

#[derive(
    FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug,
)]
#[repr(C)]
pub struct Header {
    pub version: u8,
    pub kind: u8,
    pub msg_id: U16,
}

impl Header {
    #[must_use]
    pub fn new(kind: Kind, msg_id: u16) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            kind: kind.as_u8(),
            msg_id: U16::new(msg_id),
        }
    }
}

#[derive(
    FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug,
)]
#[repr(C)]
pub struct AssignIdBody {
    pub unit_id: u8,
}

#[derive(
    FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug,
)]
#[repr(C)]
pub struct SetTimeBody {
    pub sys_time: U64,
}

#[derive(
    FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug,
)]
#[repr(C)]
pub struct UnitInfo {
    pub unit_id: u8,
    pub flags: u8,
    pub upstream_port: u8,
    pub reserved: u8,
    pub fw_version: [u8; 3],
    pub reserved2: u8,
    pub sys_time: U64,
}

#[derive(
    FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug,
)]
#[repr(C)]
pub struct FrameReply {
    pub ack: u8,
    pub data: u8,
    pub flags: u8,
    pub unit_id: u8,
    pub sys_time: U64,
}

#[derive(
    FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug,
)]
#[repr(C)]
pub struct UnblockReply {
    pub downstream_link: u8,
}

pub const HEADER_BYTES: usize = core::mem::size_of::<Header>();
pub const STATUS_BYTES: usize = 1;
pub const FRAME_REQUEST_BYTES: usize = HEADER_BYTES + HOST_TO_DEVICE_BYTES;
pub const FRAME_REPLY_BYTES: usize = HEADER_BYTES + core::mem::size_of::<FrameReply>();
pub const UNIT_INFO_REPLY_BYTES: usize =
    HEADER_BYTES + STATUS_BYTES + core::mem::size_of::<UnitInfo>();
pub const UNBLOCK_REPLY_BYTES: usize =
    HEADER_BYTES + STATUS_BYTES + core::mem::size_of::<UnblockReply>();
pub const STATUS_REPLY_BYTES: usize = HEADER_BYTES + STATUS_BYTES;
pub const MAX_REPLY_BYTES: usize = UNIT_INFO_REPLY_BYTES;

const _: () = assert!(HEADER_BYTES == 4);
const _: () = assert!(core::mem::size_of::<UnitInfo>() == 16);
const _: () = assert!(core::mem::size_of::<FrameReply>() == 12);
const _: () = assert!(FRAME_REPLY_BYTES <= MAX_REPLY_BYTES);
const _: () = assert!(UNBLOCK_REPLY_BYTES <= MAX_REPLY_BYTES);

#[must_use]
pub const fn mac(unit_id: u8) -> [u8; 6] {
    [
        MAC_PREFIX[0],
        MAC_PREFIX[1],
        MAC_PREFIX[2],
        MAC_PREFIX[3],
        MAC_PREFIX[4],
        unit_id,
    ]
}

#[must_use]
pub const fn eui64(mac: [u8; 6]) -> [u8; 8] {
    [mac[0], mac[1], mac[2], 0xFF, 0xFE, mac[3], mac[4], mac[5]]
}

#[must_use]
pub const fn link_local(mac: [u8; 6]) -> [u8; 16] {
    let id = eui64(mac);
    [
        0xFE,
        0x80,
        0,
        0,
        0,
        0,
        0,
        0,
        id[0] ^ 0x02,
        id[1],
        id[2],
        id[3],
        id[4],
        id[5],
        id[6],
        id[7],
    ]
}

#[must_use]
pub const fn solicited_node(addr: [u8; 16]) -> [u8; 16] {
    [
        0xFF, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0xFF, addr[13], addr[14], addr[15],
    ]
}

#[must_use]
pub const fn solicited_node_mac(addr: [u8; 16]) -> [u8; 6] {
    [0x33, 0x33, 0xFF, addr[13], addr[14], addr[15]]
}

#[must_use]
pub const fn unit_address(unit_id: u8) -> [u8; 16] {
    link_local(mac(unit_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_spells_ad30() {
        assert_eq!(PORT, 44336);
    }

    #[test]
    fn the_unassigned_address_is_modified_eui64_of_the_default_mac() {
        assert_eq!(mac(UNASSIGNED_ID), [0x02, 0x41, 0x55, 0x54, 0x44, 0xFF]);
        assert_eq!(
            unit_address(UNASSIGNED_ID),
            [
                0xFE, 0x80, 0, 0, 0, 0, 0, 0, 0x00, 0x41, 0x55, 0xFF, 0xFE, 0x54, 0x44, 0xFF
            ]
        );
        assert_eq!(unit_address(3)[15], 3);
    }

    #[test]
    fn solicited_node_takes_the_low_24_bits() {
        let addr = unit_address(7);
        assert_eq!(
            solicited_node(addr),
            [
                0xFF, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0xFF, 0x54, 0x44, 0x07
            ]
        );
        assert_eq!(
            solicited_node_mac(addr),
            [0x33, 0x33, 0xFF, 0x54, 0x44, 0x07]
        );
    }

    #[test]
    fn kinds_and_statuses_round_trip() {
        for raw in 0u8..=0xFF {
            if let Some(k) = Kind::from_u8(raw) {
                assert_eq!(k.as_u8(), raw);
            }
            if let Some(s) = Status::from_u8(raw) {
                assert_eq!(s.as_u8(), raw);
            }
        }
    }

    #[test]
    fn a_header_carries_the_current_version() {
        let h = Header::new(Kind::Discover, 0x1234);
        assert_eq!(h.as_bytes(), &[PROTOCOL_VERSION, 0x03, 0x34, 0x12]);
    }
}
