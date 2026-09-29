use super::Ipv6;

fn sum_bytes(mut acc: u32, bytes: &[u8]) -> u32 {
    let (pairs, rest) = bytes.as_chunks::<2>();
    for pair in pairs {
        acc += u32::from(u16::from_be_bytes(*pair));
    }
    if let [last] = rest {
        acc += u32::from(*last) << 8;
    }
    acc
}

fn fold(mut acc: u32) -> u16 {
    while acc > 0xFFFF {
        acc = (acc & 0xFFFF) + (acc >> 16);
    }
    acc as u16
}

#[must_use]
pub fn upper_layer_checksum(src: &Ipv6, dst: &Ipv6, next_header: u8, upper: &[u8]) -> u16 {
    let len = upper.len() as u32;
    let mut acc = sum_bytes(0, src);
    acc = sum_bytes(acc, dst);
    acc += len >> 16;
    acc += len & 0xFFFF;
    acc += u32::from(next_header);
    acc = sum_bytes(acc, upper);
    !fold(acc)
}
