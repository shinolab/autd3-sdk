mod build;
mod parse;

use etherparse::{Ethernet2Header, Ipv6Header, UdpHeader};

pub use build::{UDP_PAYLOAD_OFFSET, neighbor_advertisement, udp};
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

#[must_use]
pub const fn is_link_local_unicast(ip: &Ipv6) -> bool {
    ip[0] == 0xFE && ip[1] & 0xC0 == 0x80
}

#[derive(Clone, Copy)]
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
    use rstest::rstest;

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

    #[rstest]
    #[case::host(HOST_IP, true)]
    #[case::first_of_the_prefix([0xFE, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], true)]
    #[case::last_of_the_prefix([0xFE, 0xBF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], true)]
    #[case::just_below_the_prefix([0xFE, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], false)]
    #[case::site_local([0xFE, 0xC0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], false)]
    #[case::unspecified(UNSPECIFIED, false)]
    #[case::loopback([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], false)]
    #[case::all_nodes(autd3_cpu_wire::udp::ALL_NODES, false)]
    #[case::global([0x20, 0x01, 0x0D, 0xB8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], false)]
    #[case::unique_local([0xFD, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1], false)]
    fn only_fe80_slash_10_is_a_link_local_unicast_address(
        #[case] ip: Ipv6,
        #[case] expected: bool,
    ) {
        assert_eq!(is_link_local_unicast(&ip), expected);
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
    fn a_frame_cut_at_any_length_is_parsed_without_a_panic() {
        let mut ptp = [0u8; MIN_FRAME];
        ptp[12..14].copy_from_slice(&ETHERTYPE_PTP.to_be_bytes());
        for len in 0..=ptp.len() {
            assert_eq!(
                matches!(parse(&ptp[..len]), Some(Packet::Ptp { .. })),
                len >= ETH_HEADER,
                "ptp cut at {len}"
            );
        }

        let payload: Vec<u8> = (0..1446u16).map(|i| i as u8).collect();
        for frame in [
            udp_frame(&payload),
            udp_frame(&[]),
            neighbor_solicitation(UNIT_IP, NDP_HOP_LIMIT),
        ] {
            assert!(parse(&frame).is_some());
            for len in 0..frame.len() {
                assert_eq!(
                    parse(&frame[..len]),
                    None,
                    "cut at {len} of {}",
                    frame.len()
                );
            }
            let mut padded = frame.clone();
            padded.resize(MAX_FRAME, 0xA5);
            for len in frame.len()..=MAX_FRAME {
                assert_eq!(parse(&padded[..len]), parse(&frame), "padded to {len}");
            }
        }
    }

    #[test]
    fn a_frame_with_any_length_field_is_parsed_without_a_panic() {
        for frame in [
            udp_frame(&[1, 2, 3, 4]),
            neighbor_solicitation(UNIT_IP, NDP_HOP_LIMIT),
        ] {
            for length in 0..=u16::MAX {
                let mut ip = frame.clone();
                ip[ETH_HEADER + 4..][..2].copy_from_slice(&length.to_be_bytes());
                let _ = parse(&ip);
            }
        }
        let frame = udp_frame(&[1, 2, 3, 4]);
        for length in 0..=u16::MAX {
            let mut udp = frame.clone();
            udp[ETH_HEADER + IPV6_HEADER + 4..][..2].copy_from_slice(&length.to_be_bytes());
            if let Some(Packet::Udp(u)) = parse(&udp) {
                assert!(u.payload.len() <= 4, "udp length {length}");
            }
        }
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

        assert_eq!(parse(&echo_request(&[1, 2, 3, 4])), None);

        let mut reply = echo_request(&[1, 2, 3, 4]);
        reply[ETH_HEADER + IPV6_HEADER] = icmpv6::TYPE_ECHO_REPLY;
        assert_eq!(parse(&reply), None);
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
