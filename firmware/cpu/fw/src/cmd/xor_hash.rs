use zerocopy::FromBytes;

use crate::port::Port;
use crate::proto::{
    ERR_INVALID_DATA, ERR_INVALID_PAYLOAD, ERR_NONE, XOR_HASH_MAX_DATA_LEN, XorHashPayload,
};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let Ok(p) = XorHashPayload::ref_from_bytes(payload) else {
        return ERR_INVALID_PAYLOAD;
    };
    let sleep_ms = p.sleep_ms.get();
    let data_len = usize::from(p.data_len.get());
    if data_len > XOR_HASH_MAX_DATA_LEN {
        return ERR_INVALID_PAYLOAD;
    }
    if sleep_ms != 0 {
        port.sleep_ms(sleep_ms);
    }
    if p.data[..data_len].iter().fold(0u8, |h, b| h ^ b) != 0 {
        return ERR_INVALID_DATA;
    }
    ERR_NONE
}
