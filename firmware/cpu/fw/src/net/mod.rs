mod build;
mod checksum;
mod parse;

pub use build::{UDP_PAYLOAD_OFFSET, echo_reply, neighbor_advertisement, udp};
pub use checksum::upper_layer_checksum;
pub use parse::{Packet, Udp, parse};

pub type Mac = [u8; 6];
pub type Ipv6 = [u8; 16];

pub const ETH_HEADER: usize = 14;
pub const IPV6_HEADER: usize = 40;
pub const UDP_HEADER: usize = 8;
pub const MIN_FRAME: usize = 60;
pub const MAX_FRAME: usize = 1514;

pub const ETHERTYPE_IPV6: u16 = 0x86DD;
pub const ETHERTYPE_PTP: u16 = 0x88F7;

pub const NEXT_HEADER_UDP: u8 = 17;
pub const NEXT_HEADER_ICMPV6: u8 = 58;

pub const ICMPV6_ECHO_REQUEST: u8 = 128;
pub const ICMPV6_ECHO_REPLY: u8 = 129;
pub const ICMPV6_NEIGHBOR_SOLICITATION: u8 = 135;
pub const ICMPV6_NEIGHBOR_ADVERTISEMENT: u8 = 136;

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
