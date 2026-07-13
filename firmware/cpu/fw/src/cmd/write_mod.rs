use crate::fpga;
use crate::params::{ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, BRAM_SELECT_MOD, NUM_BANKS};
use crate::port::Port;
use crate::proto::{
    ERR_INVALID_PAYLOAD, ERR_NONE, MOD_BUFFER_SAMPLES, MOD_WRITE_MAX_DATA_LEN,
    MOD_WRITE_OFFSET_BANK, MOD_WRITE_OFFSET_DATA, MOD_WRITE_OFFSET_DATA_LEN,
    MOD_WRITE_OFFSET_OFFSET, read_u16, read_u32,
};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let bank = payload[MOD_WRITE_OFFSET_BANK];
    let offset = read_u32(payload, MOD_WRITE_OFFSET_OFFSET);
    let data_len = read_u16(payload, MOD_WRITE_OFFSET_DATA_LEN);

    if usize::from(bank) >= NUM_BANKS
        || !offset.is_multiple_of(2)
        || usize::from(data_len) > MOD_WRITE_MAX_DATA_LEN
        || offset > MOD_BUFFER_SAMPLES
        || u32::from(data_len) > MOD_BUFFER_SAMPLES - offset
    {
        return ERR_INVALID_PAYLOAD;
    }

    let data = &payload[MOD_WRITE_OFFSET_DATA..MOD_WRITE_OFFSET_DATA + usize::from(data_len)];
    fpga::write_ram(
        port,
        BRAM_SELECT_MOD,
        ADDR_MOD_MEM_WR_BANK,
        ADDR_MOD_MEM_WR_PAGE,
        bank,
        offset / 2,
        data,
    );
    ERR_NONE
}
