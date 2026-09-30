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
const FLAG_TWO_STEP: u8 = 0x02;
const LOG_INTERVAL_UNSPECIFIED: u8 = 0x7F;
pub const PORT_NUMBER: u16 = 1;

pub const HEADER_LEN: usize = 34;
const TIMESTAMP_LEN: usize = 10;
pub const PORT_IDENTITY_LEN: usize = 10;
pub const EVENT_LEN: usize = HEADER_LEN + TIMESTAMP_LEN;
pub const DELAY_RESP_LEN: usize = EVENT_LEN + PORT_IDENTITY_LEN;
pub const FRAME_CAP: usize = MIN_FRAME + 8;

const OFF_TYPE: usize = 0;
const OFF_VERSION: usize = 1;
const OFF_LENGTH: usize = 2;
const OFF_FLAGS: usize = 6;
const OFF_CORRECTION: usize = 8;
const OFF_IDENTITY: usize = 20;
const OFF_SEQ: usize = 30;
const OFF_CONTROL: usize = 32;
const OFF_LOG_INTERVAL: usize = 33;
const OFF_TIMESTAMP: usize = HEADER_LEN;
const OFF_REQUESTING: usize = EVENT_LEN;
const TIMESTAMP_SEC_BYTES: usize = 6;

pub type PortIdentity = [u8; PORT_IDENTITY_LEN];

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
    let header = message.get(..EVENT_LEN)?;
    if header[OFF_VERSION] & 0x0F != PTP_VERSION {
        return None;
    }
    let kind = header[OFF_TYPE] & 0x0F;
    let mut correction = [0u8; 8];
    correction.copy_from_slice(&header[OFF_CORRECTION..OFF_CORRECTION + 8]);
    let mut sec = 0u64;
    for &b in &header[OFF_TIMESTAMP..OFF_TIMESTAMP + TIMESTAMP_SEC_BYTES] {
        sec = (sec << 8) | u64::from(b);
    }
    let mut ns = [0u8; 4];
    ns.copy_from_slice(&header[OFF_TIMESTAMP + TIMESTAMP_SEC_BYTES..OFF_TIMESTAMP + TIMESTAMP_LEN]);
    let mut source = [0u8; PORT_IDENTITY_LEN];
    source.copy_from_slice(&header[OFF_IDENTITY..OFF_IDENTITY + PORT_IDENTITY_LEN]);
    let requesting = if kind == MSG_DELAY_RESP {
        let raw = message.get(OFF_REQUESTING..OFF_REQUESTING + PORT_IDENTITY_LEN)?;
        let mut id = [0u8; PORT_IDENTITY_LEN];
        id.copy_from_slice(raw);
        Some(id)
    } else {
        None
    };
    Some(Message {
        kind,
        seq: u16::from_be_bytes([header[OFF_SEQ], header[OFF_SEQ + 1]]),
        correction: i64::from_be_bytes(correction),
        timestamp: sec
            .saturating_mul(NS_PER_SEC)
            .saturating_add(u64::from(u32::from_be_bytes(ns))),
        source,
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
    buf.fill(0);
    buf[0..6].copy_from_slice(&autd3_cpu_wire::udp::PTP_MULTICAST_MAC);
    buf[6..12].copy_from_slice(&mac);
    buf[12..14].copy_from_slice(&ETHERTYPE_PTP.to_be_bytes());
    let ptp = &mut buf[ETH_HEADER..];
    ptp[OFF_TYPE] = out.kind;
    ptp[OFF_VERSION] = PTP_VERSION;
    ptp[OFF_LENGTH..OFF_LENGTH + 2].copy_from_slice(&(len as u16).to_be_bytes());
    if out.kind == MSG_SYNC {
        ptp[OFF_FLAGS] = FLAG_TWO_STEP;
    }
    ptp[OFF_CORRECTION..OFF_CORRECTION + 8].copy_from_slice(&out.correction.to_be_bytes());
    ptp[OFF_IDENTITY..OFF_IDENTITY + PORT_IDENTITY_LEN].copy_from_slice(&port_identity(clock_id));
    ptp[OFF_SEQ..OFF_SEQ + 2].copy_from_slice(&out.seq.to_be_bytes());
    ptp[OFF_CONTROL] = control;
    ptp[OFF_LOG_INTERVAL] = LOG_INTERVAL_UNSPECIFIED;
    let sec = out.timestamp / NS_PER_SEC;
    let ns = (out.timestamp % NS_PER_SEC) as u32;
    ptp[OFF_TIMESTAMP..OFF_TIMESTAMP + TIMESTAMP_SEC_BYTES]
        .copy_from_slice(&sec.to_be_bytes()[8 - TIMESTAMP_SEC_BYTES..]);
    ptp[OFF_TIMESTAMP + TIMESTAMP_SEC_BYTES..OFF_TIMESTAMP + TIMESTAMP_LEN]
        .copy_from_slice(&ns.to_be_bytes());
    if let Some(requesting) = out.requesting {
        ptp[OFF_REQUESTING..OFF_REQUESTING + PORT_IDENTITY_LEN].copy_from_slice(&requesting);
    }
    (ETH_HEADER + len).max(MIN_FRAME)
}
