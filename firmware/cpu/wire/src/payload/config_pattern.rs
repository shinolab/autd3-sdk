use zerocopy::little_endian::{U16, U32};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_header;
use crate::layout::{BUFFER_SIZE_MIN, MAX_FOCI_TOTAL};
use crate::params::{
    EMISSION_MAX_INDICES, EMISSION_TYPE_FOCI, EMISSION_TYPE_RAW, NUM_FOCI_MAX, REP_INFINITE,
};
use crate::{Error, PatternBank};

crate::wire_enum! {
    pub enum EmissionType {
        Foci = EMISSION_TYPE_FOCI,
        Raw = EMISSION_TYPE_RAW,
    }
}

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigPatternPayload {
    pub bank: PatternBank,
    pub emission_type: EmissionType,
    pub divider: U16,
    pub size: U32,
    pub num_foci: u8,
    pub reserved: u8,
    pub sound_speed: U16,
    pub rep: U16,
}

pub(super) fn validate_pattern_config(
    emission_type: EmissionType,
    divider: u16,
    size: u32,
    num_foci: u8,
    sound_speed: u16,
    rep: u16,
) -> Result<(), Error> {
    let emission_valid = match emission_type {
        EmissionType::Raw => {
            (1..=EMISSION_MAX_INDICES).contains(&size)
                && (size >= BUFFER_SIZE_MIN as u32 || rep == REP_INFINITE)
        }
        EmissionType::Foci => {
            (1..=NUM_FOCI_MAX).contains(&num_foci)
                && (BUFFER_SIZE_MIN as u32..=MAX_FOCI_TOTAL as u32 / u32::from(num_foci))
                    .contains(&size)
                && sound_speed != 0
        }
    };
    if divider == 0 || !emission_valid {
        return Err(Error::InvalidPayload);
    }
    Ok(())
}

impl ConfigPatternPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let (p, _) = try_read_header::<Self>(payload)?;
        validate_pattern_config(
            p.emission_type,
            p.divider.get(),
            p.size.get(),
            p.num_foci,
            p.sound_speed.get(),
            p.rep.get(),
        )?;
        Ok(p)
    }
}

const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, emission_type) == 1);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, divider) == 2);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, size) == 4);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, num_foci) == 8);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, sound_speed) == 10);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, rep) == 12);
const _: () = assert!(core::mem::size_of::<ConfigPatternPayload>() == 14);
