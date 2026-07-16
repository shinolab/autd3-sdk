use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::WritePatternCompressedPayload;

use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_BANKS,
    NUM_TRANSDUCERS,
};
use crate::port::Port;
use crate::proto::{EMISSION_RAM_WORDS, EMISSION_SLOT_WORDS, Error, wire_enum};

wire_enum! {
    pub enum PatternFormat {
        PhaseFull = 0x01,
        PhaseHalf = 0x02,
    }
}

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, rest)) = WritePatternCompressedPayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    let offset = p.offset.get();

    let Some(format) = PatternFormat::from_u8(p.format) else {
        return Err(Error::InvalidPayload);
    };
    let max_count = match format {
        PatternFormat::PhaseFull => 2,
        PatternFormat::PhaseHalf => 4,
    };
    if usize::from(p.bank) >= NUM_BANKS
        || p.count < 1
        || p.count > max_count
        || offset > EMISSION_RAM_WORDS
        || u32::from(p.count - 1) * EMISSION_SLOT_WORDS + NUM_TRANSDUCERS as u32
            > EMISSION_RAM_WORDS - offset
    {
        return Err(Error::InvalidPayload);
    }

    let mut slot = [0u8; 2 * NUM_TRANSDUCERS];
    for g in 0..p.count {
        for t in 0..NUM_TRANSDUCERS {
            let w = u16::from_le_bytes([rest[2 * t], rest[2 * t + 1]]);
            let phase = match format {
                PatternFormat::PhaseFull => (w >> (8 * u16::from(g))) as u8,
                PatternFormat::PhaseHalf => {
                    let p4 = ((w >> (4 * u16::from(g))) & 0x0F) as u8;
                    (p4 << 4) | p4
                }
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
    Ok(())
}
