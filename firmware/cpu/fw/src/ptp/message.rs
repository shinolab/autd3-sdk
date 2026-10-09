use etherparse::{EtherType, Ethernet2Header};
use zerocopy::big_endian::{I64, U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::net::{ETH_HEADER, ETHERTYPE_PTP, MIN_FRAME, Mac};
use crate::nic::NS_PER_SEC;

autd3_cpu_wire::wire_enum_u8! {
    pub enum MessageType {
        Sync = 0x0,
        DelayReq = 0x1,
        FollowUp = 0x8,
        DelayResp = 0x9,
    }
}

impl MessageType {
    const fn control(self) -> u8 {
        match self {
            Self::Sync => 0,
            Self::DelayReq => 1,
            Self::FollowUp => 2,
            Self::DelayResp => 3,
        }
    }

    const fn len(self) -> usize {
        match self {
            Self::DelayResp => size_of::<DelayResp>(),
            _ => size_of::<TimestampedMessage>(),
        }
    }

    const fn destination(self) -> Mac {
        match self {
            Self::Sync | Self::DelayReq => autd3_cpu_wire::udp::PTP_MULTICAST_MAC,
            Self::FollowUp | Self::DelayResp => autd3_cpu_wire::udp::PTP_GENERAL_MAC,
        }
    }
}

const PTP_VERSION: u8 = 2;
const VERSION_MASK: u8 = 0x0F;
const TYPE_MASK: u8 = 0x0F;
const FLAG_TWO_STEP: u8 = 0x02;
const LOG_INTERVAL_UNSPECIFIED: u8 = 0x7F;
const PORT_NUMBER: u16 = 1;

pub const PORT_IDENTITY_LEN: usize = 10;
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
struct TimestampedMessage {
    header: Header,
    timestamp: Timestamp,
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct DelayResp {
    base: TimestampedMessage,
    requesting: PortIdentity,
}

const _: () = assert!(size_of::<Header>() == 34);
const _: () = assert!(size_of::<Timestamp>() == 10);
const _: () = assert!(size_of::<TimestampedMessage>() == 44);
const _: () = assert!(size_of::<DelayResp>() == 54);
const _: () = assert!(ETH_HEADER + size_of::<DelayResp>() == FRAME_CAP);

impl Timestamp {
    fn from_ns(ns: u64) -> Self {
        let mut seconds = [0; TIMESTAMP_SEC_BYTES];
        seconds.copy_from_slice(&(ns / NS_PER_SEC).to_be_bytes()[8 - TIMESTAMP_SEC_BYTES..]);
        Self {
            seconds,
            nanoseconds: U32::new((ns % NS_PER_SEC) as u32),
        }
    }

    fn as_ns(&self) -> Option<u64> {
        let mut sec = [0; 8];
        sec[8 - TIMESTAMP_SEC_BYTES..].copy_from_slice(&self.seconds);
        u64::from_be_bytes(sec)
            .checked_mul(NS_PER_SEC)?
            .checked_add(u64::from(self.nanoseconds.get()))
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
    pub kind: MessageType,
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
    let (base, _) = TimestampedMessage::ref_from_prefix(message).ok()?;
    let header = &base.header;
    if header.version & VERSION_MASK != PTP_VERSION {
        return None;
    }
    let kind = MessageType::from_u8(header.message_type & TYPE_MASK)?;
    let timestamp = base.timestamp.as_ns()?;
    let requesting = if kind == MessageType::DelayResp {
        Some(DelayResp::ref_from_prefix(message).ok()?.0.requesting)
    } else {
        None
    };
    Some(Message {
        kind,
        seq: header.seq.get(),
        correction: header.correction.get(),
        timestamp,
        source: header.source,
        requesting,
    })
}

pub struct Outgoing {
    pub kind: MessageType,
    pub seq: u16,
    pub timestamp: u64,
    pub correction: i64,
    pub requesting: Option<PortIdentity>,
}

#[must_use]
pub fn build(buf: &mut [u8; FRAME_CAP], mac: Mac, clock_id: [u8; 8], out: &Outgoing) -> usize {
    let len = out.kind.len();
    let eth = Ethernet2Header {
        source: mac,
        destination: out.kind.destination(),
        ether_type: EtherType(ETHERTYPE_PTP),
    };
    let message = TimestampedMessage {
        header: Header {
            message_type: out.kind.as_u8(),
            version: PTP_VERSION,
            length: U16::new(len as u16),
            domain: 0,
            minor_sdo: 0,
            flags: [
                if out.kind == MessageType::Sync {
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
            control: out.kind.control(),
            log_interval: LOG_INTERVAL_UNSPECIFIED,
        },
        timestamp: Timestamp::from_ns(out.timestamp),
    };
    let (eth_header, body) = buf.split_at_mut(ETH_HEADER);
    let (base, requesting) = body.split_at_mut(size_of::<TimestampedMessage>());
    eth_header.copy_from_slice(&eth.to_bytes());
    base.copy_from_slice(message.as_bytes());
    requesting.copy_from_slice(&out.requesting.unwrap_or_default());
    (ETH_HEADER + len).max(MIN_FRAME)
}

const ETHERTYPE_MAC_CONTROL: u16 = 0x8808;
const MAC_CONTROL_PAUSE: u16 = 0x0001;

#[must_use]
pub fn build_pause(buf: &mut [u8; FRAME_CAP], mac: Mac, quanta: u16) -> usize {
    let eth = Ethernet2Header {
        source: mac,
        destination: autd3_cpu_wire::udp::MAC_CONTROL_MAC,
        ether_type: EtherType(ETHERTYPE_MAC_CONTROL),
    };
    buf.fill(0);
    let (eth_header, body) = buf.split_at_mut(ETH_HEADER);
    eth_header.copy_from_slice(&eth.to_bytes());
    body[..2].copy_from_slice(&MAC_CONTROL_PAUSE.to_be_bytes());
    body[2..4].copy_from_slice(&quanta.to_be_bytes());
    MIN_FRAME
}

#[cfg(test)]
mod tests {
    use super::{FRAME_CAP, MessageType, Outgoing, build, build_pause, correction_ns, parse};
    use crate::net::{ETH_HEADER, Mac};
    use crate::nic::NS_PER_SEC;
    use rstest::rstest;

    const SLAVE_MAC: Mac = [0x02, 0x41, 0x55, 0x54, 0x44, 0x01];

    #[test]
    fn messages_round_trip() {
        let mut buf = [0u8; FRAME_CAP];
        let out = Outgoing {
            kind: MessageType::DelayResp,
            seq: 0xBEEF,
            timestamp: 812_345_678_901_234_567,
            correction: -(5 << 16),
            requesting: Some([1, 2, 3, 4, 5, 6, 7, 8, 0, 1]),
        };
        let len = build(&mut buf, SLAVE_MAC, [7; 8], &out);
        assert_eq!(len, 68);
        assert_eq!(&buf[0..6], &autd3_cpu_wire::udp::PTP_GENERAL_MAC);
        assert_eq!(&buf[12..14], &[0x88, 0xF7]);
        let m = parse(&buf[ETH_HEADER..len]).unwrap();
        assert_eq!(m.kind, MessageType::DelayResp);
        assert_eq!(m.seq, 0xBEEF);
        assert_eq!(m.timestamp, 812_345_678_901_234_567);
        assert_eq!(correction_ns(m.correction), -5);
        assert_eq!(m.requesting, Some([1, 2, 3, 4, 5, 6, 7, 8, 0, 1]));
        assert_eq!(&m.source[..8], &[7; 8]);

        let sync = Outgoing {
            kind: MessageType::Sync,
            seq: 1,
            timestamp: 0,
            correction: 0,
            requesting: None,
        };
        let len = build(&mut buf, SLAVE_MAC, [7; 8], &sync);
        assert_eq!(len, 60);
        assert_eq!(&buf[0..6], &autd3_cpu_wire::udp::PTP_MULTICAST_MAC);
        assert_eq!(buf[ETH_HEADER + 6], 0x02);
        assert_eq!(parse(&buf[ETH_HEADER..len]).unwrap().requesting, None);
        assert_eq!(parse(&buf[ETH_HEADER..][..20]), None);
        let mut v1 = buf;
        v1[ETH_HEADER + 1] = 1;
        assert_eq!(parse(&v1[ETH_HEADER..len]), None);
    }

    #[test]
    fn a_timestamp_beyond_the_nanosecond_range_is_rejected() {
        let mut buf = [0u8; FRAME_CAP];
        let out = Outgoing {
            kind: MessageType::FollowUp,
            seq: 1,
            timestamp: u64::MAX,
            correction: 0,
            requesting: None,
        };
        let len = build(&mut buf, SLAVE_MAC, [7; 8], &out);
        assert_eq!(parse(&buf[ETH_HEADER..len]).unwrap().timestamp, u64::MAX);
        let nanoseconds = &mut buf[ETH_HEADER + 40..][..4];
        let carried = u32::from_be_bytes(nanoseconds.try_into().unwrap()) + 1;
        nanoseconds.copy_from_slice(&carried.to_be_bytes());
        assert_eq!(parse(&buf[ETH_HEADER..len]), None);
        buf[ETH_HEADER + 34..][..6].fill(0xFF);
        assert_eq!(parse(&buf[ETH_HEADER..len]), None);
    }

    #[test]
    fn a_pause_frame_matches_the_golden_bytes() {
        let mut buf = [0xCCu8; FRAME_CAP];
        let len = build_pause(&mut buf, SLAVE_MAC, 48);
        assert_eq!(len, 60);
        assert_eq!(hex(&buf[..18]), "0180c2000001024155544401880800010030");
        assert!(buf[18..len].iter().all(|&b| b == 0));
    }

    fn hex(bytes: &[u8]) -> std::string::String {
        use core::fmt::Write;
        let mut s = std::string::String::new();
        for b in bytes {
            write!(s, "{b:02x}").unwrap();
        }
        s
    }

    #[rstest]
    #[case::sync(
        MessageType::Sync,
        None,
        "011b1900000002415554440188f70002002c00000200fffffffffffafffd0000000001020304050607080001beef007f0003123456783ade68b100000000000000000000"
    )]
    #[case::follow_up(
        MessageType::FollowUp,
        None,
        "03415554440002415554440188f70802002c00000000fffffffffffafffd0000000001020304050607080001beef027f0003123456783ade68b100000000000000000000"
    )]
    #[case::delay_req(
        MessageType::DelayReq,
        None,
        "011b1900000002415554440188f70102002c00000000fffffffffffafffd0000000001020304050607080001beef017f0003123456783ade68b100000000000000000000"
    )]
    #[case::delay_resp(
        MessageType::DelayResp,
        Some([1, 2, 3, 4, 5, 6, 7, 8, 0, 1]),
        "03415554440002415554440188f70902003600000000fffffffffffafffd0000000001020304050607080001beef037f0003123456783ade68b101020304050607080001"
    )]
    fn every_message_kind_matches_the_golden_bytes(
        #[case] kind: MessageType,
        #[case] requesting: Option<[u8; 10]>,
        #[case] golden: &str,
    ) {
        let mut buf = [0xCCu8; FRAME_CAP];
        let out = Outgoing {
            kind,
            seq: 0xBEEF,
            timestamp: 0x0003_1234_5678 * NS_PER_SEC + 987_654_321,
            correction: -(5 << 16) - 3,
            requesting,
        };
        let len = build(&mut buf, SLAVE_MAC, [1, 2, 3, 4, 5, 6, 7, 8], &out);
        assert_eq!(hex(&buf), golden);
        let m = parse(&buf[ETH_HEADER..len]).unwrap();
        assert_eq!(m.kind, kind);
        assert_eq!(m.seq, 0xBEEF);
        assert_eq!(m.timestamp, out.timestamp);
        assert_eq!(m.correction, out.correction);
        assert_eq!(m.requesting, requesting);
        assert_eq!(m.source, [1, 2, 3, 4, 5, 6, 7, 8, 0, 1]);
    }
}
