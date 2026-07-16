use core::mem::{offset_of, size_of};

use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::fpga;
use crate::params::{ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, BRAM_SELECT_MOD, NUM_BANKS};
use crate::port::Port;
use crate::proto::{Error, MOD_BUFFER_SAMPLES, PAYLOAD_BYTES};

pub const MOD_WRITE_MAX_DATA_LEN: usize = PAYLOAD_BYTES - 8;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WriteModPayload {
    pub bank: u8,
    _reserved: u8,
    pub offset: U32,
    pub data_len: U16,
    pub data: [u8; MOD_WRITE_MAX_DATA_LEN],
}

const _: () = assert!(size_of::<WriteModPayload>() == PAYLOAD_BYTES);
const _: () = assert!(offset_of!(WriteModPayload, bank) == 0);
const _: () = assert!(offset_of!(WriteModPayload, offset) == 2);
const _: () = assert!(offset_of!(WriteModPayload, data_len) == 6);
const _: () = assert!(offset_of!(WriteModPayload, data) == 8);

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok(p) = WriteModPayload::ref_from_bytes(payload) else {
        return Err(Error::InvalidPayload);
    };
    let offset = p.offset.get();
    let data_len = p.data_len.get();

    if usize::from(p.bank) >= NUM_BANKS
        || !offset.is_multiple_of(2)
        || usize::from(data_len) > MOD_WRITE_MAX_DATA_LEN
        || offset > MOD_BUFFER_SAMPLES
        || u32::from(data_len) > MOD_BUFFER_SAMPLES - offset
    {
        return Err(Error::InvalidPayload);
    }

    fpga::write_ram(
        port,
        BRAM_SELECT_MOD,
        ADDR_MOD_MEM_WR_BANK,
        ADDR_MOD_MEM_WR_PAGE,
        p.bank,
        offset / 2,
        &p.data[..usize::from(data_len)],
    );
    Ok(())
}
