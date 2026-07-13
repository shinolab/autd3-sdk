use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_BANKS,
    NUM_TRANSDUCERS,
};
use crate::port::Port;
use crate::proto::{
    EM_COMPRESSED_OFFSET_BANK, EM_COMPRESSED_OFFSET_COUNT, EM_COMPRESSED_OFFSET_DATA,
    EM_COMPRESSED_OFFSET_FORMAT, EM_COMPRESSED_OFFSET_OFFSET, EMISSION_RAM_WORDS,
    EMISSION_SLOT_WORDS, ERR_INVALID_PAYLOAD, ERR_NONE, WRITE_PATTERN_FORMAT_PHASE_FULL,
    WRITE_PATTERN_FORMAT_PHASE_HALF, read_u32,
};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let bank = payload[EM_COMPRESSED_OFFSET_BANK];
    let format = payload[EM_COMPRESSED_OFFSET_FORMAT];
    let count = payload[EM_COMPRESSED_OFFSET_COUNT];
    let offset = read_u32(payload, EM_COMPRESSED_OFFSET_OFFSET);

    let max_count = if format == WRITE_PATTERN_FORMAT_PHASE_FULL {
        2
    } else {
        4
    };
    if usize::from(bank) >= NUM_BANKS
        || !(WRITE_PATTERN_FORMAT_PHASE_FULL..=WRITE_PATTERN_FORMAT_PHASE_HALF).contains(&format)
        || count < 1
        || count > max_count
        || offset > EMISSION_RAM_WORDS
        || u32::from(count - 1) * EMISSION_SLOT_WORDS + NUM_TRANSDUCERS as u32
            > EMISSION_RAM_WORDS - offset
    {
        return ERR_INVALID_PAYLOAD;
    }

    let data = &payload[EM_COMPRESSED_OFFSET_DATA..EM_COMPRESSED_OFFSET_DATA + 2 * NUM_TRANSDUCERS];
    let mut slot = [0u8; 2 * NUM_TRANSDUCERS];
    for g in 0..count {
        for t in 0..NUM_TRANSDUCERS {
            let w = u16::from_le_bytes([data[2 * t], data[2 * t + 1]]);
            let phase = if format == WRITE_PATTERN_FORMAT_PHASE_FULL {
                (w >> (8 * u16::from(g))) as u8
            } else {
                let p4 = ((w >> (4 * u16::from(g))) & 0x0F) as u8;
                (p4 << 4) | p4
            };
            slot[2 * t] = phase;
            slot[2 * t + 1] = 0xFF;
        }
        fpga::write_ram(
            port,
            BRAM_SELECT_EMISSION,
            ADDR_PATTERN_MEM_WR_BANK,
            ADDR_PATTERN_MEM_WR_PAGE,
            bank,
            offset + u32::from(g) * EMISSION_SLOT_WORDS,
            &slot,
        );
    }
    ERR_NONE
}
