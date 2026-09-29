use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::WritePatternCompressedPayload;

use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_BANKS,
    NUM_TRANSDUCERS,
};
use crate::port::Port;
use crate::proto::{EMISSION_RAM_WORDS, EMISSION_SLOT_WORDS, Error, wire_enum};
use autd3_cpu_wire::layout::PATTERN_COMPRESSED_MAX_GROUPS;

wire_enum! {
    pub enum PatternFormat {
        PhaseFull = 0x01,
        PhaseHalf = 0x02,
    }
}

impl PatternFormat {
    #[must_use]
    pub const fn patterns_per_word(self) -> u8 {
        match self {
            Self::PhaseFull => 2,
            Self::PhaseHalf => 4,
        }
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
    let per_word = format.patterns_per_word();
    let max_count = per_word * PATTERN_COMPRESSED_MAX_GROUPS as u8;
    let groups = usize::from(p.count.div_ceil(per_word));
    if usize::from(p.bank) >= NUM_BANKS
        || p.count < 1
        || p.count > max_count
        || rest.len() < groups * 2 * NUM_TRANSDUCERS
        || offset
            .saturating_add(u32::from(p.count - 1) * EMISSION_SLOT_WORDS + NUM_TRANSDUCERS as u32)
            > EMISSION_RAM_WORDS
    {
        return Err(Error::InvalidPayload);
    }

    let mut slot = [0u8; 2 * NUM_TRANSDUCERS];
    for g in 0..p.count {
        let group = &rest[usize::from(g / per_word) * 2 * NUM_TRANSDUCERS..];
        let sub = u16::from(g % per_word);
        for t in 0..NUM_TRANSDUCERS {
            let w = u16::from_le_bytes([group[2 * t], group[2 * t + 1]]);
            let phase = match format {
                PatternFormat::PhaseFull => (w >> (8 * sub)) as u8,
                PatternFormat::PhaseHalf => {
                    let p4 = ((w >> (4 * sub)) & 0x0F) as u8;
                    (p4 << 4) | p4
                }
            };
            slot[2 * t] = phase;
            slot[2 * t + 1] = p.intensity;
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
