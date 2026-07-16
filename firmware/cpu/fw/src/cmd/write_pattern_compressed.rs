use core::mem::{offset_of, size_of};

use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_BANKS,
    NUM_TRANSDUCERS,
};
use crate::port::Port;
use crate::proto::{EMISSION_RAM_WORDS, EMISSION_SLOT_WORDS, Error, PAYLOAD_BYTES, wire_enum};

wire_enum! {
    pub enum PatternFormat {
        PhaseFull = 0x01,
        PhaseHalf = 0x02,
    }
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternCompressedPayload {
    pub bank: u8,
    pub format: u8,
    pub count: u8,
    _reserved: u8,
    pub offset: U32,
    pub data: [u8; NUM_TRANSDUCERS * 2],
}

const _: () = assert!(size_of::<WritePatternCompressedPayload>() <= PAYLOAD_BYTES);
const _: () = assert!(offset_of!(WritePatternCompressedPayload, bank) == 0);
const _: () = assert!(offset_of!(WritePatternCompressedPayload, format) == 1);
const _: () = assert!(offset_of!(WritePatternCompressedPayload, count) == 2);
const _: () = assert!(offset_of!(WritePatternCompressedPayload, offset) == 4);
const _: () = assert!(offset_of!(WritePatternCompressedPayload, data) == 8);

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, _)) = WritePatternCompressedPayload::ref_from_prefix(payload) else {
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
            let w = u16::from_le_bytes([p.data[2 * t], p.data[2 * t + 1]]);
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
