use etherparse::{
    EtherType, Ethernet2Header, IcmpEchoHeader, Icmpv6Header, Icmpv6Type, IpNumber, Ipv6FlowLabel,
    Ipv6Header, UdpHeader, icmpv6::NeighborAdvertisementHeader,
};

use zerocopy::{Immutable, IntoBytes, KnownLayout, Unaligned};

use super::{
    DEFAULT_HOP_LIMIT, ECHO_ID_SEQ, ETH_HEADER, Endpoint, IPV6_HEADER, Ipv6, MIN_FRAME, Mac,
    NDP_HOP_LIMIT, UDP_HEADER,
};

pub const UDP_PAYLOAD_OFFSET: usize = ETH_HEADER + IPV6_HEADER + UDP_HEADER;

const IP_OFFSET: usize = ETH_HEADER;
const UPPER_OFFSET: usize = ETH_HEADER + IPV6_HEADER;
const ICMPV6_HEADER: usize = 8;
const NDP_OPTION_TARGET_LINK_LAYER: u8 = 2;
const NDP_OPTION_UNITS: u8 = 1;
const NA_LEN: usize = ICMPV6_HEADER + size_of::<NaBody>();

#[derive(IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
struct NaBody {
    target: Ipv6,
    option_type: u8,
    option_units: u8,
    link_layer_address: Mac,
}

const _: () = assert!(size_of::<NaBody>() == 24);

struct Headers {
    src_mac: Mac,
    dst_mac: Mac,
    src_ip: Ipv6,
    dst_ip: Ipv6,
    next_header: IpNumber,
    hop_limit: u8,
}

fn write_headers(buf: &mut [u8], h: &Headers, upper_len: usize) -> Ipv6Header {
    let eth = Ethernet2Header {
        source: h.src_mac,
        destination: h.dst_mac,
        ether_type: EtherType::IPV6,
    };
    let ip = Ipv6Header {
        traffic_class: 0,
        flow_label: Ipv6FlowLabel::ZERO,
        payload_length: upper_len as u16,
        next_header: h.next_header,
        hop_limit: h.hop_limit,
        source: h.src_ip,
        destination: h.dst_ip,
    };
    buf[..ETH_HEADER].copy_from_slice(&eth.to_bytes());
    buf[IP_OFFSET..UPPER_OFFSET].copy_from_slice(&ip.to_bytes());
    ip
}

fn write_icmpv6(
    buf: &mut [u8],
    ip: &Ipv6Header,
    icmp_type: Icmpv6Type,
    upper_len: usize,
) -> Option<()> {
    let upper = &mut buf[UPPER_OFFSET..][..upper_len];
    let header = Icmpv6Header::with_checksum(
        icmp_type,
        ip.source,
        ip.destination,
        &upper[ICMPV6_HEADER..],
    )
    .ok()?;
    upper[..ICMPV6_HEADER].copy_from_slice(&header.to_bytes());
    Some(())
}

fn finish(buf: &mut [u8], upper_len: usize) -> usize {
    let len = UPPER_OFFSET + upper_len;
    if len < MIN_FRAME {
        buf[len..MIN_FRAME].fill(0);
        MIN_FRAME
    } else {
        len
    }
}

fn fits(buf: &[u8], upper_len: usize) -> bool {
    UPPER_OFFSET + upper_len <= buf.len()
        && u16::try_from(upper_len).is_ok()
        && buf.len() >= MIN_FRAME
}

#[must_use]
pub fn udp(buf: &mut [u8], src: &Endpoint, dst: &Endpoint, payload_len: usize) -> Option<usize> {
    let upper_len = UDP_HEADER + payload_len;
    if !fits(buf, upper_len) {
        return None;
    }
    let ip = write_headers(
        buf,
        &Headers {
            src_mac: src.mac,
            dst_mac: dst.mac,
            src_ip: src.ip,
            dst_ip: dst.ip,
            next_header: IpNumber::UDP,
            hop_limit: DEFAULT_HOP_LIMIT,
        },
        upper_len,
    );
    let payload = &buf[UDP_PAYLOAD_OFFSET..][..payload_len];
    let header = UdpHeader::with_ipv6_checksum(src.port, dst.port, &ip, payload).ok()?;
    buf[UPPER_OFFSET..UDP_PAYLOAD_OFFSET].copy_from_slice(&header.to_bytes());
    Some(finish(buf, upper_len))
}

#[must_use]
pub fn neighbor_advertisement(
    buf: &mut [u8],
    own_mac: Mac,
    own_ip: Ipv6,
    dst_mac: Mac,
    dst_ip: Ipv6,
) -> Option<usize> {
    if !fits(buf, NA_LEN) {
        return None;
    }
    let ip = write_headers(
        buf,
        &Headers {
            src_mac: own_mac,
            dst_mac,
            src_ip: own_ip,
            dst_ip,
            next_header: IpNumber::IPV6_ICMP,
            hop_limit: NDP_HOP_LIMIT,
        },
        NA_LEN,
    );
    let body = NaBody {
        target: own_ip,
        option_type: NDP_OPTION_TARGET_LINK_LAYER,
        option_units: NDP_OPTION_UNITS,
        link_layer_address: own_mac,
    };
    buf[UPPER_OFFSET..][ICMPV6_HEADER..NA_LEN].copy_from_slice(body.as_bytes());
    write_icmpv6(
        buf,
        &ip,
        Icmpv6Type::NeighborAdvertisement(NeighborAdvertisementHeader {
            router: false,
            solicited: true,
            r#override: true,
        }),
        NA_LEN,
    )?;
    Some(finish(buf, NA_LEN))
}

#[must_use]
pub fn echo_reply(
    buf: &mut [u8],
    own_mac: Mac,
    own_ip: Ipv6,
    dst_mac: Mac,
    dst_ip: Ipv6,
    body: &[u8],
) -> Option<usize> {
    let (id_seq, data) = body.split_first_chunk::<ECHO_ID_SEQ>()?;
    let upper_len = ICMPV6_HEADER + data.len();
    if !fits(buf, upper_len) {
        return None;
    }
    let ip = write_headers(
        buf,
        &Headers {
            src_mac: own_mac,
            dst_mac,
            src_ip: own_ip,
            dst_ip,
            next_header: IpNumber::IPV6_ICMP,
            hop_limit: DEFAULT_HOP_LIMIT,
        },
        upper_len,
    );
    buf[UPPER_OFFSET..][ICMPV6_HEADER..upper_len].copy_from_slice(data);
    write_icmpv6(
        buf,
        &ip,
        Icmpv6Type::EchoReply(IcmpEchoHeader::from_bytes(*id_seq)),
        upper_len,
    )?;
    Some(finish(buf, upper_len))
}
