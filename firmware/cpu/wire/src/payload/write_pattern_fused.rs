use zerocopy::little_endian::{U16, U32, U64};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::config_pattern::validate_pattern_config;
use super::{EmissionType, TransitionMode, try_read_header};
use crate::layout::{PATTERN_FUSED_MAX_DATA_LEN, PATTERN_RAW_DATA_LEN};
use crate::{Error, PatternBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternFusedPayload {
    pub bank: PatternBank,
    pub emission_type: EmissionType,
    pub divider: U16,
    pub size: U32,
    pub num_foci: u8,
    pub transition_mode: TransitionMode,
    pub sound_speed: U16,
    pub rep: U16,
    pub transition_value: U64,
    pub margin_ns: U32,
    pub reserved: [u8; 6],
}

impl WritePatternFusedPayload {
    pub fn parse(payload: &[u8]) -> Result<(Self, &[u8]), Error> {
        let (p, data) = try_read_header::<Self>(payload)?;
        let data_len = data.len();
        if !data_len.is_multiple_of(2)
            || data_len > PATTERN_FUSED_MAX_DATA_LEN
            || (p.emission_type == EmissionType::Raw
                && (p.size.get() != 1 || data_len != PATTERN_RAW_DATA_LEN))
        {
            return Err(Error::InvalidPayload);
        }
        validate_pattern_config(
            p.emission_type,
            p.divider.get(),
            p.size.get(),
            p.num_foci,
            p.sound_speed.get(),
            p.rep.get(),
        )?;
        Ok((p, data))
    }
}

const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, emission_type) == 1);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, divider) == 2);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, size) == 4);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, num_foci) == 8);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, transition_mode) == 9);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, sound_speed) == 10);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, rep) == 12);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, transition_value) == 14);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, margin_ns) == 22);
const _: () = assert!(core::mem::offset_of!(WritePatternFusedPayload, reserved) == 26);
const _: () = assert!(core::mem::size_of::<WritePatternFusedPayload>() == 32);
