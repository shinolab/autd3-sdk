use zerocopy::FromBytes;

use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{
    EM_WRITE_MAX_DATA_LEN, EMISSION_RAM_WORDS, ERR_INVALID_PAYLOAD, ERR_NONE, WritePatternPayload,
};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let Ok(p) = WritePatternPayload::ref_from_bytes(payload) else {
        return ERR_INVALID_PAYLOAD;
    };
    let offset = p.offset.get();
    let data_len = p.data_len.get();

    if usize::from(p.bank) >= NUM_BANKS
        || !data_len.is_multiple_of(2)
        || usize::from(data_len) > EM_WRITE_MAX_DATA_LEN
        || offset > EMISSION_RAM_WORDS
        || u32::from(data_len / 2) > EMISSION_RAM_WORDS - offset
    {
        return ERR_INVALID_PAYLOAD;
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
    ERR_NONE
}
