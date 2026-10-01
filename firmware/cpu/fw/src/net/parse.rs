use etherparse::{
    EtherType, Ethernet2Slice, Icmpv6Slice, Icmpv6Type, IpNumber, Ipv6HeaderSlice, Ipv6Slice,
    UdpSlice,
};

use super::{ETHERTYPE_PTP, Ipv6, Mac};

const ICMPV6_ECHO_ID_SEQ: usize = 4;
const NS_TARGET: usize = 16;

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

#[must_use]
pub fn parse(frame: &[u8]) -> Option<Packet<'_>> {
    let eth = Ethernet2Slice::from_slice_without_fcs(frame).ok()?;
    let src_mac = eth.source();
    match eth.ether_type() {
        EtherType(ETHERTYPE_PTP) => Some(Packet::Ptp {
            src_mac,
            message: eth.payload_slice(),
        }),
        EtherType::IPV6 => parse_ipv6(src_mac, eth.payload_slice()),
        _ => None,
    }
}

fn parse_ipv6(src_mac: Mac, frame: &[u8]) -> Option<Packet<'_>> {
    let header = Ipv6HeaderSlice::from_slice(frame).ok()?;
    if header.payload_length() == 0
        || !matches!(header.next_header(), IpNumber::UDP | IpNumber::IPV6_ICMP)
    {
        return None;
    }
    let ip = Ipv6Slice::from_slice(frame).ok()?;
    let upper = ip.payload();
    let src_ip = header.source();
    let dst_ip = header.destination();
    match upper.ip_number {
        IpNumber::UDP => {
            let udp = UdpSlice::from_slice(upper.payload).ok()?;
            if udp.length() == 0 {
                return None;
            }
            Some(Packet::Udp(Udp {
                src_mac,
                src_ip,
                dst_ip,
                src_port: udp.source_port(),
                dst_port: udp.destination_port(),
                payload: udp.payload(),
            }))
        }
        IpNumber::IPV6_ICMP => {
            let icmp = Icmpv6Slice::from_slice(upper.payload).ok()?;
            match icmp.icmp_type() {
                Icmpv6Type::NeighborSolicitation => Some(Packet::NeighborSolicitation {
                    src_mac,
                    src_ip,
                    dst_ip,
                    target: icmp.payload().get(..NS_TARGET)?.try_into().ok()?,
                    hop_limit: header.hop_limit(),
                }),
                Icmpv6Type::EchoRequest(_) => Some(Packet::EchoRequest {
                    src_mac,
                    src_ip,
                    dst_ip,
                    body: icmp.slice().get(ICMPV6_ECHO_ID_SEQ..)?,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}
