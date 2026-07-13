use zerocopy::FromBytes;

use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_BANKS,
    NUM_TRANSDUCERS,
};
use crate::port::Port;
use crate::proto::{
    EMISSION_RAM_WORDS, EMISSION_SLOT_WORDS, ERR_INVALID_PAYLOAD, ERR_NONE,
    WRITE_PATTERN_FORMAT_PHASE_FULL, WRITE_PATTERN_FORMAT_PHASE_HALF,
    WritePatternCompressedPayload,
};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let Ok((p, _)) = WritePatternCompressedPayload::ref_from_prefix(payload) else {
        return ERR_INVALID_PAYLOAD;
    };
    let offset = p.offset.get();

    let max_count = if p.format == WRITE_PATTERN_FORMAT_PHASE_FULL {
        2
    } else {
        4
    };
    if usize::from(p.bank) >= NUM_BANKS
        || !(WRITE_PATTERN_FORMAT_PHASE_FULL..=WRITE_PATTERN_FORMAT_PHASE_HALF).contains(&p.format)
        || p.count < 1
        || p.count > max_count
        || offset > EMISSION_RAM_WORDS
        || u32::from(p.count - 1) * EMISSION_SLOT_WORDS + NUM_TRANSDUCERS as u32
            > EMISSION_RAM_WORDS - offset
    {
        return ERR_INVALID_PAYLOAD;
    }

    let mut slot = [0u8; 2 * NUM_TRANSDUCERS];
    for g in 0..p.count {
        for t in 0..NUM_TRANSDUCERS {
            let w = u16::from_le_bytes([p.data[2 * t], p.data[2 * t + 1]]);
            let phase = if p.format == WRITE_PATTERN_FORMAT_PHASE_FULL {
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
            p.bank,
            offset + u32::from(g) * EMISSION_SLOT_WORDS,
            &slot,
        );
    }
    ERR_NONE
}
