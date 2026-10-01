mod build;
mod parse;

use etherparse::{EtherType, Ethernet2Header, IpNumber, Ipv6Header, UdpHeader, icmpv6};

pub use build::{UDP_PAYLOAD_OFFSET, echo_reply, neighbor_advertisement, udp};
pub use parse::{Packet, Udp, parse};

pub type Mac = [u8; 6];
pub type Ipv6 = [u8; 16];

pub const ETH_HEADER: usize = Ethernet2Header::LEN;
pub const IPV6_HEADER: usize = Ipv6Header::LEN;
pub const UDP_HEADER: usize = UdpHeader::LEN;
pub const MIN_FRAME: usize = 60;
pub const MAX_FRAME: usize = 1514;

pub const ETHERTYPE_IPV6: u16 = EtherType::IPV6.0;
pub const ETHERTYPE_PTP: u16 = 0x88F7;

pub const NEXT_HEADER_UDP: u8 = IpNumber::UDP.0;
pub const NEXT_HEADER_ICMPV6: u8 = IpNumber::IPV6_ICMP.0;

pub const ICMPV6_ECHO_REQUEST: u8 = icmpv6::TYPE_ECHO_REQUEST;
pub const ICMPV6_ECHO_REPLY: u8 = icmpv6::TYPE_ECHO_REPLY;
pub const ICMPV6_NEIGHBOR_SOLICITATION: u8 = icmpv6::TYPE_NEIGHBOR_SOLICITATION;
pub const ICMPV6_NEIGHBOR_ADVERTISEMENT: u8 = icmpv6::TYPE_NEIGHBOR_ADVERTISEMENT;

pub const NDP_HOP_LIMIT: u8 = 255;
pub const DEFAULT_HOP_LIMIT: u8 = 64;

pub const UNSPECIFIED: Ipv6 = [0; 16];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Endpoint {
    pub mac: Mac,
    pub ip: Ipv6,
    pub port: u16,
}

#[cfg(test)]
mod tests;
