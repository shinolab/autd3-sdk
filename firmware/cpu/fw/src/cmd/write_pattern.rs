use core::mem::{offset_of, size_of};

use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{EMISSION_RAM_WORDS, Error, PAYLOAD_BYTES};

pub const EM_WRITE_MAX_DATA_LEN: usize = PAYLOAD_BYTES - 8;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternPayload {
    pub bank: u8,
    _reserved: u8,
    pub offset: U32,
    pub data_len: U16,
    pub data: [u8; EM_WRITE_MAX_DATA_LEN],
}

const _: () = assert!(size_of::<WritePatternPayload>() == PAYLOAD_BYTES);
const _: () = assert!(offset_of!(WritePatternPayload, bank) == 0);
const _: () = assert!(offset_of!(WritePatternPayload, offset) == 2);
const _: () = assert!(offset_of!(WritePatternPayload, data_len) == 6);
const _: () = assert!(offset_of!(WritePatternPayload, data) == 8);
const _: () = assert!(crate::params::NUM_TRANSDUCERS * 2 <= EM_WRITE_MAX_DATA_LEN);

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok(p) = WritePatternPayload::ref_from_bytes(payload) else {
        return Err(Error::InvalidPayload);
    };
    let offset = p.offset.get();
    let data_len = p.data_len.get();

    if usize::from(p.bank) >= NUM_BANKS
        || !data_len.is_multiple_of(2)
        || usize::from(data_len) > EM_WRITE_MAX_DATA_LEN
        || offset > EMISSION_RAM_WORDS
        || u32::from(data_len / 2) > EMISSION_RAM_WORDS - offset
    {
        return Err(Error::InvalidPayload);
    }

    fpga::write_ram(
        port,
        BRAM_SELECT_EMISSION,
        ADDR_PATTERN_MEM_WR_BANK,
        ADDR_PATTERN_MEM_WR_PAGE,
        p.bank,
        offset,
        &p.data[..usize::from(data_len)],
    );
    Ok(())
}
