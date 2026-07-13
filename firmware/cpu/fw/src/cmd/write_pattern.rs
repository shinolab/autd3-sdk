use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{
    EM_WRITE_MAX_DATA_LEN, EM_WRITE_OFFSET_BANK, EM_WRITE_OFFSET_DATA, EM_WRITE_OFFSET_DATA_LEN,
    EM_WRITE_OFFSET_OFFSET, EMISSION_RAM_WORDS, ERR_INVALID_PAYLOAD, ERR_NONE, read_u16, read_u32,
};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let bank = payload[EM_WRITE_OFFSET_BANK];
    let offset = read_u32(payload, EM_WRITE_OFFSET_OFFSET);
    let data_len = read_u16(payload, EM_WRITE_OFFSET_DATA_LEN);

    if usize::from(bank) >= NUM_BANKS
        || !data_len.is_multiple_of(2)
        || usize::from(data_len) > EM_WRITE_MAX_DATA_LEN
        || offset > EMISSION_RAM_WORDS
        || u32::from(data_len / 2) > EMISSION_RAM_WORDS - offset
    {
        return ERR_INVALID_PAYLOAD;
    }

    let data = &payload[EM_WRITE_OFFSET_DATA..EM_WRITE_OFFSET_DATA + usize::from(data_len)];
    fpga::write_ram(
        port,
        BRAM_SELECT_EMISSION,
        ADDR_PATTERN_MEM_WR_BANK,
        ADDR_PATTERN_MEM_WR_PAGE,
        bank,
        offset,
        data,
    );
    ERR_NONE
}
