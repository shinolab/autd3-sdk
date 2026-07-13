use zerocopy::FromBytes;

use crate::fpga;
use crate::params::{ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, BRAM_SELECT_MOD, NUM_BANKS};
use crate::port::Port;
use crate::proto::{
    ERR_INVALID_PAYLOAD, ERR_NONE, MOD_BUFFER_SAMPLES, MOD_WRITE_MAX_DATA_LEN, WriteModPayload,
};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let Ok(p) = WriteModPayload::ref_from_bytes(payload) else {
        return ERR_INVALID_PAYLOAD;
    };
    let offset = p.offset.get();
    let data_len = p.data_len.get();

    if usize::from(p.bank) >= NUM_BANKS
        || !offset.is_multiple_of(2)
        || usize::from(data_len) > MOD_WRITE_MAX_DATA_LEN
        || offset > MOD_BUFFER_SAMPLES
        || u32::from(data_len) > MOD_BUFFER_SAMPLES - offset
    {
        return ERR_INVALID_PAYLOAD;
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
    ERR_NONE
}
