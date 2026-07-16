const fn parse_u8(s: &str) -> u8 {
    let bytes = s.as_bytes();
    let mut n = 0u8;
    let mut i = 0;
    while i < bytes.len() {
        n = n * 10 + (bytes[i] - b'0');
        i += 1;
    }
    n
}

pub const FW_VERSION_MAJOR: u8 = parse_u8(env!("CARGO_PKG_VERSION_MAJOR"));
pub const FW_VERSION_MINOR: u8 = parse_u8(env!("CARGO_PKG_VERSION_MINOR"));
pub const FW_VERSION_PATCH: u8 = parse_u8(env!("CARGO_PKG_VERSION_PATCH"));
