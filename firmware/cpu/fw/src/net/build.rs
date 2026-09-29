use super::{
    DEFAULT_HOP_LIMIT, ETH_HEADER, ETHERTYPE_IPV6, Endpoint, ICMPV6_ECHO_REPLY,
    ICMPV6_NEIGHBOR_ADVERTISEMENT, IPV6_HEADER, Ipv6, MIN_FRAME, Mac, NDP_HOP_LIMIT,
    NEXT_HEADER_ICMPV6, NEXT_HEADER_UDP, UDP_HEADER, upper_layer_checksum,
};

pub const UDP_PAYLOAD_OFFSET: usize = ETH_HEADER + IPV6_HEADER + UDP_HEADER;

const IP_OFFSET: usize = ETH_HEADER;
const UPPER_OFFSET: usize = ETH_HEADER + IPV6_HEADER;
const NA_FLAGS_SOLICITED_OVERRIDE: u8 = 0x60;
const NDP_OPTION_TARGET_LINK_LAYER: u8 = 2;
const NA_LEN: usize = 32;
const ECHO_HEADER: usize = 4;

struct Headers {
    src_mac: Mac,
    dst_mac: Mac,
    src_ip: Ipv6,
    dst_ip: Ipv6,
    next_header: u8,
    hop_limit: u8,
}

fn write_headers(buf: &mut [u8], ip: &Headers, upper_len: usize) {
    buf[0..6].copy_from_slice(&ip.dst_mac);
    buf[6..12].copy_from_slice(&ip.src_mac);
    buf[12..14].copy_from_slice(&ETHERTYPE_IPV6.to_be_bytes());
    let h = &mut buf[IP_OFFSET..UPPER_OFFSET];
    h[0..4].copy_from_slice(&[0x60, 0, 0, 0]);
    h[4..6].copy_from_slice(&(upper_len as u16).to_be_bytes());
    h[6] = ip.next_header;
    h[7] = ip.hop_limit;
    h[8..24].copy_from_slice(&ip.src_ip);
    h[24..40].copy_from_slice(&ip.dst_ip);
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
    write_headers(
        buf,
        &Headers {
            src_mac: src.mac,
            dst_mac: dst.mac,
            src_ip: src.ip,
            dst_ip: dst.ip,
            next_header: NEXT_HEADER_UDP,
            hop_limit: DEFAULT_HOP_LIMIT,
        },
        upper_len,
    );
    let upper = &mut buf[UPPER_OFFSET..UPPER_OFFSET + upper_len];
    upper[0..2].copy_from_slice(&src.port.to_be_bytes());
    upper[2..4].copy_from_slice(&dst.port.to_be_bytes());
    upper[4..6].copy_from_slice(&(upper_len as u16).to_be_bytes());
    upper[6..8].fill(0);
    let sum = match upper_layer_checksum(&src.ip, &dst.ip, NEXT_HEADER_UDP, upper) {
        0 => 0xFFFF,
        sum => sum,
    };
    upper[6..8].copy_from_slice(&sum.to_be_bytes());
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
    write_headers(
        buf,
        &Headers {
            src_mac: own_mac,
            dst_mac,
            src_ip: own_ip,
            dst_ip,
            next_header: NEXT_HEADER_ICMPV6,
            hop_limit: NDP_HOP_LIMIT,
        },
        NA_LEN,
    );
    let upper = &mut buf[UPPER_OFFSET..UPPER_OFFSET + NA_LEN];
    upper.fill(0);
    upper[0] = ICMPV6_NEIGHBOR_ADVERTISEMENT;
    upper[4] = NA_FLAGS_SOLICITED_OVERRIDE;
    upper[8..24].copy_from_slice(&own_ip);
    upper[24] = NDP_OPTION_TARGET_LINK_LAYER;
    upper[25] = 1;
    upper[26..32].copy_from_slice(&own_mac);
    let sum = upper_layer_checksum(&own_ip, &dst_ip, NEXT_HEADER_ICMPV6, upper);
    upper[2..4].copy_from_slice(&sum.to_be_bytes());
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
    let upper_len = ECHO_HEADER + body.len();
    if !fits(buf, upper_len) {
        return None;
    }
    write_headers(
        buf,
        &Headers {
            src_mac: own_mac,
            dst_mac,
            src_ip: own_ip,
            dst_ip,
            next_header: NEXT_HEADER_ICMPV6,
            hop_limit: DEFAULT_HOP_LIMIT,
        },
        upper_len,
    );
    let upper = &mut buf[UPPER_OFFSET..UPPER_OFFSET + upper_len];
    upper[0] = ICMPV6_ECHO_REPLY;
    upper[1] = 0;
    upper[2..4].fill(0);
    upper[ECHO_HEADER..].copy_from_slice(body);
    let sum = upper_layer_checksum(&own_ip, &dst_ip, NEXT_HEADER_ICMPV6, upper);
    upper[2..4].copy_from_slice(&sum.to_be_bytes());
    Some(finish(buf, upper_len))
}
