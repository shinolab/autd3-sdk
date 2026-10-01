use etherparse::{EtherType, Ethernet2Header};
use zerocopy::big_endian::{I64, U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::net::{ETH_HEADER, ETHERTYPE_PTP, MIN_FRAME, Mac};
use crate::nic::NS_PER_SEC;

pub const MSG_SYNC: u8 = 0x0;
pub const MSG_DELAY_REQ: u8 = 0x1;
pub const MSG_FOLLOW_UP: u8 = 0x8;
pub const MSG_DELAY_RESP: u8 = 0x9;

const CONTROL_SYNC: u8 = 0;
const CONTROL_DELAY_REQ: u8 = 1;
const CONTROL_FOLLOW_UP: u8 = 2;
const CONTROL_DELAY_RESP: u8 = 3;

const PTP_VERSION: u8 = 2;
const VERSION_MASK: u8 = 0x0F;
const TYPE_MASK: u8 = 0x0F;
const FLAG_TWO_STEP: u8 = 0x02;
const LOG_INTERVAL_UNSPECIFIED: u8 = 0x7F;
pub const PORT_NUMBER: u16 = 1;

pub const PORT_IDENTITY_LEN: usize = 10;
pub const HEADER_LEN: usize = size_of::<Header>();
pub const EVENT_LEN: usize = size_of::<EventMessage>();
pub const DELAY_RESP_LEN: usize = size_of::<DelayResp>();
pub const FRAME_CAP: usize = MIN_FRAME + 8;

const TIMESTAMP_SEC_BYTES: usize = 6;

pub type PortIdentity = [u8; PORT_IDENTITY_LEN];

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct Header {
    message_type: u8,
    version: u8,
    length: U16,
    domain: u8,
    minor_sdo: u8,
    flags: [u8; 2],
    correction: I64,
    type_specific: [u8; 4],
    source: PortIdentity,
    seq: U16,
    control: u8,
    log_interval: u8,
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct Timestamp {
    seconds: [u8; TIMESTAMP_SEC_BYTES],
    nanoseconds: U32,
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct EventMessage {
    header: Header,
    origin: Timestamp,
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct DelayResp {
    event: EventMessage,
    requesting: PortIdentity,
}

const _: () = assert!(HEADER_LEN == 34);
const _: () = assert!(size_of::<Timestamp>() == 10);
const _: () = assert!(EVENT_LEN == 44);
const _: () = assert!(DELAY_RESP_LEN == 54);
const _: () = assert!(ETH_HEADER + DELAY_RESP_LEN == FRAME_CAP);

impl Timestamp {
    fn from_ns(ns: u64) -> Self {
        let mut seconds = [0; TIMESTAMP_SEC_BYTES];
        seconds.copy_from_slice(&(ns / NS_PER_SEC).to_be_bytes()[8 - TIMESTAMP_SEC_BYTES..]);
        Self {
            seconds,
            nanoseconds: U32::new((ns % NS_PER_SEC) as u32),
        }
    }

    fn as_ns(&self) -> u64 {
        let mut sec = [0; 8];
        sec[8 - TIMESTAMP_SEC_BYTES..].copy_from_slice(&self.seconds);
        u64::from_be_bytes(sec)
            .saturating_mul(NS_PER_SEC)
            .saturating_add(u64::from(self.nanoseconds.get()))
    }
}

#[must_use]
pub fn port_identity(clock_id: [u8; 8]) -> PortIdentity {
    let mut id = [0; PORT_IDENTITY_LEN];
    id[..8].copy_from_slice(&clock_id);
    id[8..].copy_from_slice(&PORT_NUMBER.to_be_bytes());
    id
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Message {
    pub kind: u8,
    pub seq: u16,
    pub correction: i64,
    pub timestamp: u64,
    pub source: PortIdentity,
    pub requesting: Option<PortIdentity>,
}

#[must_use]
pub fn correction_ns(raw: i64) -> i64 {
    raw >> 16
}

#[must_use]
pub fn parse(message: &[u8]) -> Option<Message> {
    let (event, _) = EventMessage::ref_from_prefix(message).ok()?;
    let header = &event.header;
    if header.version & VERSION_MASK != PTP_VERSION {
        return None;
    }
    let kind = header.message_type & TYPE_MASK;
    let requesting = if kind == MSG_DELAY_RESP {
        Some(DelayResp::ref_from_prefix(message).ok()?.0.requesting)
    } else {
        None
    };
    Some(Message {
        kind,
        seq: header.seq.get(),
        correction: header.correction.get(),
        timestamp: event.origin.as_ns(),
        source: header.source,
        requesting,
    })
}

pub struct Outgoing {
    pub kind: u8,
    pub seq: u16,
    pub timestamp: u64,
    pub correction: i64,
    pub requesting: Option<PortIdentity>,
}

#[must_use]
pub fn build(buf: &mut [u8; FRAME_CAP], mac: Mac, clock_id: [u8; 8], out: &Outgoing) -> usize {
    let (len, control) = match out.kind {
        MSG_SYNC => (EVENT_LEN, CONTROL_SYNC),
        MSG_FOLLOW_UP => (EVENT_LEN, CONTROL_FOLLOW_UP),
        MSG_DELAY_REQ => (EVENT_LEN, CONTROL_DELAY_REQ),
        _ => (DELAY_RESP_LEN, CONTROL_DELAY_RESP),
    };
    let eth = Ethernet2Header {
        source: mac,
        destination: autd3_cpu_wire::udp::PTP_MULTICAST_MAC,
        ether_type: EtherType(ETHERTYPE_PTP),
    };
    let message = DelayResp {
        event: EventMessage {
            header: Header {
                message_type: out.kind,
                version: PTP_VERSION,
                length: U16::new(len as u16),
                domain: 0,
                minor_sdo: 0,
                flags: [
                    if out.kind == MSG_SYNC {
                        FLAG_TWO_STEP
                    } else {
                        0
                    },
                    0,
                ],
                correction: I64::new(out.correction),
                type_specific: [0; 4],
                source: port_identity(clock_id),
                seq: U16::new(out.seq),
                control,
                log_interval: LOG_INTERVAL_UNSPECIFIED,
            },
            origin: Timestamp::from_ns(out.timestamp),
        },
        requesting: out.requesting.unwrap_or_default(),
    };
    buf[..ETH_HEADER].copy_from_slice(&eth.to_bytes());
    buf[ETH_HEADER..].copy_from_slice(message.as_bytes());
    (ETH_HEADER + len).max(MIN_FRAME)
}
