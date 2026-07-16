use core::mem::{offset_of, size_of};

use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::port::Port;
use crate::proto::{Error, PAYLOAD_BYTES};

pub const XOR_HASH_MAX_DATA_LEN: usize = PAYLOAD_BYTES - 4;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct XorHashPayload {
    pub sleep_ms: U16,
    pub data_len: U16,
    pub data: [u8; XOR_HASH_MAX_DATA_LEN],
}

const _: () = assert!(size_of::<XorHashPayload>() == PAYLOAD_BYTES);
const _: () = assert!(offset_of!(XorHashPayload, sleep_ms) == 0);
const _: () = assert!(offset_of!(XorHashPayload, data_len) == 2);
const _: () = assert!(offset_of!(XorHashPayload, data) == 4);

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok(p) = XorHashPayload::ref_from_bytes(payload) else {
        return Err(Error::InvalidPayload);
    };
    let sleep_ms = p.sleep_ms.get();
    let data_len = usize::from(p.data_len.get());
    if data_len > XOR_HASH_MAX_DATA_LEN {
        return Err(Error::InvalidPayload);
    }
    if sleep_ms != 0 {
        port.sleep_ms(sleep_ms);
    }
    if p.data[..data_len].iter().fold(0u8, |h, b| h ^ b) != 0 {
        return Err(Error::InvalidData);
    }
    Ok(())
}
