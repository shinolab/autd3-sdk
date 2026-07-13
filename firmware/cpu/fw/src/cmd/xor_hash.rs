use crate::port::Port;
use crate::proto::{
    ERR_INVALID_DATA, ERR_INVALID_PAYLOAD, ERR_NONE, XOR_HASH_MAX_DATA_LEN, XOR_HASH_OFFSET_DATA,
    XOR_HASH_OFFSET_DATA_LEN, XOR_HASH_OFFSET_SLEEP_MS, read_u16,
};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let sleep_ms = read_u16(payload, XOR_HASH_OFFSET_SLEEP_MS);
    let data_len = read_u16(payload, XOR_HASH_OFFSET_DATA_LEN) as usize;
    if data_len > XOR_HASH_MAX_DATA_LEN {
        return ERR_INVALID_PAYLOAD;
    }
    if sleep_ms != 0 {
        port.sleep_ms(sleep_ms);
    }
    let data = &payload[XOR_HASH_OFFSET_DATA..XOR_HASH_OFFSET_DATA + data_len];
    if data.iter().fold(0u8, |h, b| h ^ b) != 0 {
        return ERR_INVALID_DATA;
    }
    ERR_NONE
}
