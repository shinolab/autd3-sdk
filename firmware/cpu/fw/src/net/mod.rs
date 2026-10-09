mod build;
mod parse;

use etherparse::{Ethernet2Header, Ipv6Header, UdpHeader};

pub use build::{UDP_PAYLOAD_OFFSET, echo_reply, neighbor_advertisement, udp};
pub use parse::{Packet, Udp, parse};

pub type Mac = [u8; 6];
pub type Ipv6 = [u8; 16];

pub const ETH_HEADER: usize = Ethernet2Header::LEN;
pub const IPV6_HEADER: usize = Ipv6Header::LEN;
pub const UDP_HEADER: usize = UdpHeader::LEN;
pub const MIN_FRAME: usize = 60;
pub const MAX_FRAME: usize = 1514;

pub const ETHERTYPE_PTP: u16 = 0x88F7;

pub const NDP_HOP_LIMIT: u8 = 255;
pub const DEFAULT_HOP_LIMIT: u8 = 64;

pub const UNSPECIFIED: Ipv6 = [0; 16];

const ECHO_ID_SEQ: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Endpoint {
    pub mac: Mac,
    pub ip: Ipv6,
    pub port: u16,
}

#[cfg(test)]
mod tests {
    use std::vec;
    use std::vec::Vec;

    use etherparse::{EtherType, IpNumber, icmpv6};

    use super::*;

    const HOST_MAC: Mac = [0x3C, 0x7C, 0x3F, 0x11, 0x22, 0x33];
    const HOST_IP: Ipv6 = [
        0xFE, 0x80, 0, 0, 0, 0, 0, 0, 0x3E, 0x7C, 0x3F, 0xFF, 0xFE, 0x11, 0x22, 0x33,
    ];
    const UNIT_MAC: Mac = [0x02, 0x41, 0x55, 0x54, 0x44, 0x01];
    const UNIT_IP: Ipv6 = [
        0xFE, 0x80, 0, 0, 0, 0, 0, 0, 0x00, 0x41, 0x55, 0xFF, 0xFE, 0x54, 0x44, 0x01,
    ];

    fn host() -> Endpoint {
        Endpoint {
            mac: HOST_MAC,
            ip: HOST_IP,
            port: 50000,
        }
    }

    fn unit() -> Endpoint {
        Endpoint {
            mac: UNIT_MAC,
            ip: UNIT_IP,
            port: 44336,
        }
    }

    fn udp_frame(payload: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; MAX_FRAME];
        buf[UDP_PAYLOAD_OFFSET..][..payload.len()].copy_from_slice(payload);
        let len = udp(&mut buf, &host(), &unit(), payload.len()).unwrap();
        buf.truncate(len);
        buf
    }

    fn upper(frame: &[u8]) -> &[u8] {
        let len = usize::from(u16::from_be_bytes([frame[18], frame[19]]));
        &frame[ETH_HEADER + IPV6_HEADER..][..len]
    }

    fn upper_layer_checksum(src: &Ipv6, dst: &Ipv6, next_header: u8, upper: &[u8]) -> u16 {
        let mut words: Vec<u8> = Vec::new();
        words.extend_from_slice(src);
        words.extend_from_slice(dst);
        words.extend_from_slice(&(upper.len() as u32).to_be_bytes());
        words.extend_from_slice(&[0, 0, 0, next_header]);
        words.extend_from_slice(upper);
        if words.len() % 2 == 1 {
            words.push(0);
        }
        let mut sum: u64 = words
            .chunks(2)
            .map(|w| u64::from(u16::from_be_bytes([w[0], w[1]])))
            .sum();
        while sum > 0xFFFF {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        !(sum as u16)
    }

    fn tag_overwritten(mut frame: Vec<u8>) -> Vec<u8> {
        frame[0..4].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        frame[4] = 0x80;
        frame
    }

    #[test]
    fn a_udp_datagram_round_trips_through_the_parser() {
        let payload: Vec<u8> = (0..630u16).map(|i| i as u8).collect();
        let frame = tag_overwritten(udp_frame(&payload));
        let Some(Packet::Udp(u)) = parse(&frame) else {
            panic!("not udp");
        };
        assert_eq!(u.src_mac, HOST_MAC);
        assert_eq!(u.src_ip, HOST_IP);
        assert_eq!(u.dst_ip, UNIT_IP);
        assert_eq!(u.src_port, 50000);
        assert_eq!(u.dst_port, 44336);
        assert_eq!(u.payload, &payload[..]);
    }

    #[test]
    fn the_udp_checksum_verifies_to_zero() {
        for len in [0usize, 1, 2, 3, 16, 17, 630] {
            let payload: Vec<u8> = (0..len).map(|i| (i * 31 + 7) as u8).collect();
            let frame = udp_frame(&payload);
            assert_eq!(
                upper_layer_checksum(&HOST_IP, &UNIT_IP, IpNumber::UDP.0, upper(&frame)),
                0,
                "len {len}"
            );
            assert_ne!(&frame[ETH_HEADER + IPV6_HEADER + 6..][..2], &[0, 0]);
        }
    }

    #[test]
    fn the_checksum_matches_an_independent_sum() {
        let payload = [0xDE, 0xAD, 0xBE, 0xEF, 0x01];
        let frame = udp_frame(&payload);
        let up = upper(&frame);
        let mut body = up.to_vec();
        body[6] = 0;
        body[7] = 0;
        let expected = upper_layer_checksum(&HOST_IP, &UNIT_IP, IpNumber::UDP.0, &body);
        assert_eq!(u16::from_be_bytes([up[6], up[7]]), expected);
    }

    #[test]
    fn trailing_padding_is_not_payload() {
        let mut frame = udp_frame(&[1, 2, 3]);
        frame.extend_from_slice(&[0xAA; 12]);
        let Some(Packet::Udp(u)) = parse(&frame) else {
            panic!("not udp");
        };
        assert_eq!(u.payload, &[1, 2, 3]);
    }

    #[test]
    fn a_datagram_that_does_not_fit_is_refused() {
        let mut buf = vec![0u8; 100];
        assert_eq!(udp(&mut buf, &host(), &unit(), 100), None);
    }

    fn neighbor_solicitation(target: Ipv6, hop_limit: u8) -> Vec<u8> {
        let mut frame = vec![0u8; ETH_HEADER + IPV6_HEADER + 32];
        frame[0..6].copy_from_slice(&[0x33, 0x33, 0xFF, 0x54, 0x44, 0x01]);
        frame[6..12].copy_from_slice(&HOST_MAC);
        frame[12..14].copy_from_slice(&EtherType::IPV6.0.to_be_bytes());
        let ip = &mut frame[ETH_HEADER..];
        ip[0] = 0x60;
        ip[4..6].copy_from_slice(&32u16.to_be_bytes());
        ip[6] = IpNumber::IPV6_ICMP.0;
        ip[7] = hop_limit;
        ip[8..24].copy_from_slice(&HOST_IP);
        ip[24..40].copy_from_slice(&autd3_cpu_wire::udp::solicited_node(UNIT_IP));
        let icmp = &mut ip[IPV6_HEADER..];
        icmp[0] = icmpv6::TYPE_NEIGHBOR_SOLICITATION;
        icmp[8..24].copy_from_slice(&target);
        icmp[24] = 1;
        icmp[25] = 1;
        icmp[26..32].copy_from_slice(&HOST_MAC);
        frame
    }

    #[test]
    fn a_neighbor_solicitation_is_parsed() {
        let frame = neighbor_solicitation(UNIT_IP, 255);
        assert_eq!(
            parse(&frame),
            Some(Packet::NeighborSolicitation {
                src_mac: HOST_MAC,
                src_ip: HOST_IP,
                dst_ip: autd3_cpu_wire::udp::solicited_node(UNIT_IP),
                target: UNIT_IP,
                hop_limit: 255,
            })
        );
    }

    #[test]
    fn a_neighbor_advertisement_carries_the_link_layer_address() {
        let mut buf = vec![0u8; 128];
        let len = neighbor_advertisement(&mut buf, UNIT_MAC, UNIT_IP, HOST_MAC, HOST_IP).unwrap();
        let frame = &buf[..len];
        assert_eq!(&frame[0..6], &HOST_MAC);
        assert_eq!(&frame[6..12], &UNIT_MAC);
        assert_eq!(frame[ETH_HEADER + 7], NDP_HOP_LIMIT);
        let icmp = upper(frame);
        assert_eq!(icmp[0], icmpv6::TYPE_NEIGHBOR_ADVERTISEMENT);
        assert_eq!(icmp[4], 0x60);
        assert_eq!(&icmp[8..24], &UNIT_IP);
        assert_eq!(&icmp[24..26], &[2, 1]);
        assert_eq!(&icmp[26..32], &UNIT_MAC);
        assert_eq!(
            upper_layer_checksum(&UNIT_IP, &HOST_IP, IpNumber::IPV6_ICMP.0, icmp),
            0
        );
    }

    #[test]
    fn an_echo_reply_mirrors_the_request_body() {
        let body = [0x12, 0x34, 0x00, 0x07, b'p', b'i', b'n', b'g', b'!'];
        let mut req = vec![0u8; ETH_HEADER + IPV6_HEADER + 4 + body.len()];
        req[6..12].copy_from_slice(&HOST_MAC);
        req[12..14].copy_from_slice(&EtherType::IPV6.0.to_be_bytes());
        let ip = &mut req[ETH_HEADER..];
        ip[0] = 0x60;
        ip[4..6].copy_from_slice(&((4 + body.len()) as u16).to_be_bytes());
        ip[6] = IpNumber::IPV6_ICMP.0;
        ip[7] = 64;
        ip[8..24].copy_from_slice(&HOST_IP);
        ip[24..40].copy_from_slice(&UNIT_IP);
        ip[IPV6_HEADER] = icmpv6::TYPE_ECHO_REQUEST;
        ip[IPV6_HEADER + 4..].copy_from_slice(&body);
        let Some(Packet::EchoRequest {
            body: parsed,
            src_ip,
            ..
        }) = parse(&req)
        else {
            panic!("not an echo request");
        };
        assert_eq!(parsed, &body);
        assert_eq!(src_ip, HOST_IP);

        let mut buf = vec![0u8; 256];
        let len = echo_reply(&mut buf, UNIT_MAC, UNIT_IP, HOST_MAC, HOST_IP, parsed).unwrap();
        let icmp = upper(&buf[..len]);
        assert_eq!(icmp[0], icmpv6::TYPE_ECHO_REPLY);
        assert_eq!(&icmp[4..], &body);
        assert_eq!(
            upper_layer_checksum(&UNIT_IP, &HOST_IP, IpNumber::IPV6_ICMP.0, icmp),
            0
        );
    }

    #[test]
    fn a_ptp_frame_is_recognised_by_its_ethertype() {
        let mut frame = vec![0u8; 60];
        frame[12..14].copy_from_slice(&ETHERTYPE_PTP.to_be_bytes());
        frame[14] = 0x08;
        let Some(Packet::Ptp { message }) = parse(&frame) else {
            panic!("not ptp");
        };
        assert_eq!(message[0], 0x08);
    }

    #[test]
    fn malformed_frames_are_dropped() {
        assert_eq!(parse(&[]), None);
        assert_eq!(parse(&[0u8; 13]), None);
        let mut ipv4 = vec![0u8; 60];
        ipv4[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
        assert_eq!(parse(&ipv4), None);

        let mut frame = udp_frame(&[1, 2, 3, 4]);
        frame[ETH_HEADER] = 0x40;
        assert_eq!(parse(&frame), None);

        let mut frame = udp_frame(&[1, 2, 3, 4]);
        frame[ETH_HEADER..][4..6].copy_from_slice(&1000u16.to_be_bytes());
        assert_eq!(parse(&frame), None);

        let mut frame = udp_frame(&[1, 2, 3, 4]);
        frame[ETH_HEADER + IPV6_HEADER..][4..6].copy_from_slice(&4u16.to_be_bytes());
        assert_eq!(parse(&frame), None);

        let mut frame = udp_frame(&[1, 2, 3, 4]);
        frame[ETH_HEADER + 6] = 6;
        assert_eq!(parse(&frame), None);
    }

    fn hex(bytes: &[u8]) -> std::string::String {
        use core::fmt::Write;
        let mut s = std::string::String::new();
        for b in bytes {
            write!(s, "{b:02x}").unwrap();
        }
        s
    }

    const GOLDEN_UDP: &str = "3c7c3f11223302415554440186dd60000000000d1140fe80000000000000004155fffe544401fe800000000000003e7c3ffffe112233ad30c350000d92604155544433";
    const GOLDEN_NA: &str = "3c7c3f11223302415554440186dd6000000000203afffe80000000000000004155fffe544401fe800000000000003e7c3ffffe1122338800ae9c60000000fe80000000000000004155fffe5444010201024155544401";
    const GOLDEN_ECHO_REPLY: &str = "3c7c3f11223302415554440186dd60000000000d3a40fe80000000000000004155fffe544401fe800000000000003e7c3ffffe112233810038531234000770696e6721";

    #[test]
    fn a_udp_frame_matches_the_golden_bytes() {
        let payload = b"AUTD3";
        let mut buf = vec![0xCCu8; 128];
        buf[UDP_PAYLOAD_OFFSET..][..payload.len()].copy_from_slice(payload);
        let len = udp(&mut buf, &unit(), &host(), payload.len()).unwrap();
        assert_eq!(hex(&buf[..len]), GOLDEN_UDP);
    }

    #[test]
    fn a_neighbor_advertisement_matches_the_golden_bytes() {
        let mut buf = vec![0xCCu8; 128];
        let len = neighbor_advertisement(&mut buf, UNIT_MAC, UNIT_IP, HOST_MAC, HOST_IP).unwrap();
        assert_eq!(hex(&buf[..len]), GOLDEN_NA);
    }

    #[test]
    fn an_echo_reply_matches_the_golden_bytes() {
        let body = [0x12, 0x34, 0x00, 0x07, b'p', b'i', b'n', b'g', b'!'];
        let mut buf = vec![0xCCu8; 128];
        let len = echo_reply(&mut buf, UNIT_MAC, UNIT_IP, HOST_MAC, HOST_IP, &body).unwrap();
        assert_eq!(hex(&buf[..len]), GOLDEN_ECHO_REPLY);
    }

    fn echo_request(body: &[u8]) -> Vec<u8> {
        let mut req = vec![0u8; ETH_HEADER + IPV6_HEADER + 4 + body.len()];
        req[6..12].copy_from_slice(&HOST_MAC);
        req[12..14].copy_from_slice(&EtherType::IPV6.0.to_be_bytes());
        let ip = &mut req[ETH_HEADER..];
        ip[0] = 0x60;
        ip[4..6].copy_from_slice(&((4 + body.len()) as u16).to_be_bytes());
        ip[6] = IpNumber::IPV6_ICMP.0;
        ip[7] = 64;
        ip[8..24].copy_from_slice(&HOST_IP);
        ip[24..40].copy_from_slice(&UNIT_IP);
        ip[IPV6_HEADER] = icmpv6::TYPE_ECHO_REQUEST;
        ip[IPV6_HEADER + 4..].copy_from_slice(body);
        req
    }

    #[test]
    fn short_or_unknown_icmpv6_messages_are_dropped() {
        let mut short_ns = neighbor_solicitation(UNIT_IP, 255);
        short_ns[ETH_HEADER..][4..6].copy_from_slice(&23u16.to_be_bytes());
        assert_eq!(parse(&short_ns), None);

        assert_eq!(parse(&echo_request(&[1, 2, 3])), None);
        assert!(matches!(
            parse(&echo_request(&[1, 2, 3, 4])),
            Some(Packet::EchoRequest {
                body: [1, 2, 3, 4],
                ..
            })
        ));

        let mut reply = echo_request(&[1, 2, 3, 4]);
        reply[ETH_HEADER + IPV6_HEADER] = icmpv6::TYPE_ECHO_REPLY;
        assert_eq!(parse(&reply), None);
    }

    #[test]
    fn a_padded_echo_request_ends_at_the_ipv6_payload() {
        let mut req = echo_request(&[9, 8, 7, 6]);
        req.extend_from_slice(&[0xAA; 6]);
        assert!(matches!(
            parse(&req),
            Some(Packet::EchoRequest {
                body: [9, 8, 7, 6],
                ..
            })
        ));
    }

    #[test]
    fn a_udp_length_beyond_the_ipv6_payload_is_dropped() {
        let mut frame = udp_frame(&[1, 2, 3, 4]);
        frame[ETH_HEADER + IPV6_HEADER..][4..6].copy_from_slice(&13u16.to_be_bytes());
        assert_eq!(parse(&frame), None);
    }

    #[test]
    fn a_bare_ptp_header_is_still_ptp() {
        let mut frame = vec![0u8; ETH_HEADER];
        frame[12..14].copy_from_slice(&ETHERTYPE_PTP.to_be_bytes());
        assert!(matches!(parse(&frame), Some(Packet::Ptp { message: [] })));
    }

    #[test]
    fn zero_lengths_are_not_read_as_jumbograms() {
        let mut frame = udp_frame(&[1, 2, 3, 4]);
        frame[ETH_HEADER..][4..6].copy_from_slice(&0u16.to_be_bytes());
        assert_eq!(parse(&frame), None);

        let mut frame = udp_frame(&[1, 2, 3, 4]);
        frame[ETH_HEADER + IPV6_HEADER..][4..6].copy_from_slice(&0u16.to_be_bytes());
        assert_eq!(parse(&frame), None);
    }

    #[test]
    fn icmpv6_with_a_nonzero_code_is_dropped() {
        let mut ns = neighbor_solicitation(UNIT_IP, 255);
        ns[ETH_HEADER + IPV6_HEADER + 1] = 1;
        assert_eq!(parse(&ns), None);

        let mut req = echo_request(&[1, 2, 3, 4]);
        req[ETH_HEADER + IPV6_HEADER + 1] = 1;
        assert_eq!(parse(&req), None);
    }

    #[test]
    fn an_echo_body_shorter_than_its_identifier_is_not_answered() {
        let mut buf = vec![0u8; 128];
        assert_eq!(
            echo_reply(&mut buf, UNIT_MAC, UNIT_IP, HOST_MAC, HOST_IP, &[1, 2, 3]),
            None
        );
    }

    #[test]
    fn packets_with_extension_headers_are_dropped() {
        let mut frame = udp_frame(&[1, 2, 3, 4]);
        frame[ETH_HEADER + 6] = 60;
        assert_eq!(parse(&frame), None);

        let mut frame = udp_frame(&[1, 2, 3, 4]);
        frame[ETH_HEADER + 6] = 44;
        assert_eq!(parse(&frame), None);
    }
}
