use super::{
    ETH_HEADER, ETHERTYPE_IPV6, ETHERTYPE_PTP, ICMPV6_ECHO_REQUEST, ICMPV6_NEIGHBOR_SOLICITATION,
    IPV6_HEADER, Ipv6, Mac, NEXT_HEADER_ICMPV6, NEXT_HEADER_UDP, UDP_HEADER,
};

const ICMPV6_HEADER: usize = 4;
const NS_BODY: usize = 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Udp<'a> {
    pub src_mac: Mac,
    pub src_ip: Ipv6,
    pub dst_ip: Ipv6,
    pub src_port: u16,
    pub dst_port: u16,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Packet<'a> {
    Ptp {
        src_mac: Mac,
        message: &'a [u8],
    },
    Udp(Udp<'a>),
    NeighborSolicitation {
        src_mac: Mac,
        src_ip: Ipv6,
        dst_ip: Ipv6,
        target: Ipv6,
        hop_limit: u8,
    },
    EchoRequest {
        src_mac: Mac,
        src_ip: Ipv6,
        dst_ip: Ipv6,
        body: &'a [u8],
    },
}

fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

fn array<const N: usize>(bytes: &[u8], at: usize) -> Option<[u8; N]> {
    bytes.get(at..at + N)?.try_into().ok()
}

#[must_use]
pub fn parse(frame: &[u8]) -> Option<Packet<'_>> {
    let src_mac: Mac = array(frame, 6)?;
    let ethertype = be16(frame, 12)?;
    let body = frame.get(ETH_HEADER..)?;
    match ethertype {
        ETHERTYPE_PTP => Some(Packet::Ptp {
            src_mac,
            message: body,
        }),
        ETHERTYPE_IPV6 => parse_ipv6(src_mac, body),
        _ => None,
    }
}

fn parse_ipv6(src_mac: Mac, ip: &[u8]) -> Option<Packet<'_>> {
    if ip.first()? >> 4 != 6 {
        return None;
    }
    let payload_len = usize::from(be16(ip, 4)?);
    let next_header = *ip.get(6)?;
    let hop_limit = *ip.get(7)?;
    let src_ip: Ipv6 = array(ip, 8)?;
    let dst_ip: Ipv6 = array(ip, 24)?;
    let upper = ip.get(IPV6_HEADER..IPV6_HEADER + payload_len)?;
    match next_header {
        NEXT_HEADER_UDP => {
            let len = usize::from(be16(upper, 4)?);
            if len < UDP_HEADER {
                return None;
            }
            Some(Packet::Udp(Udp {
                src_mac,
                src_ip,
                dst_ip,
                src_port: be16(upper, 0)?,
                dst_port: be16(upper, 2)?,
                payload: upper.get(UDP_HEADER..len)?,
            }))
        }
        NEXT_HEADER_ICMPV6 => match *upper.first()? {
            ICMPV6_NEIGHBOR_SOLICITATION if upper.len() >= ICMPV6_HEADER + NS_BODY => {
                Some(Packet::NeighborSolicitation {
                    src_mac,
                    src_ip,
                    dst_ip,
                    target: array(upper, ICMPV6_HEADER + 4)?,
                    hop_limit,
                })
            }
            ICMPV6_ECHO_REQUEST if upper.len() >= ICMPV6_HEADER + 4 => Some(Packet::EchoRequest {
                src_mac,
                src_ip,
                dst_ip,
                body: upper.get(ICMPV6_HEADER..)?,
            }),
            _ => None,
        },
        _ => None,
    }
}
