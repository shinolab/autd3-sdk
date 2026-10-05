use core::num::{NonZeroU8, NonZeroU16};

use zerocopy::little_endian::{U16, U32};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_exact;
use crate::fpga_params::{EMISSION_MAX_INDICES, EmissionType, NUM_FOCI_MAX, REP_INFINITE};
use crate::layout::{BUFFER_SIZE_MIN, MAX_FOCI_TOTAL};
use crate::{Error, PatternBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigPatternPayload {
    pub bank: PatternBank,
    pub emission_type: EmissionType,
    pub divider: U16,
    pub size: U32,
    pub num_foci: Option<NonZeroU8>,
    pub reserved: u8,
    pub sound_speed: U16,
    pub rep: U16,
}

impl ConfigPatternPayload {
    #[must_use]
    pub const fn divider(&self) -> Option<NonZeroU16> {
        NonZeroU16::new(self.divider.get())
    }

    #[must_use]
    pub const fn sound_speed(&self) -> Option<NonZeroU16> {
        NonZeroU16::new(self.sound_speed.get())
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p = try_read_exact::<Self>(payload)?;
        p.divider().ok_or(Error::InvalidPayload)?;
        let size = p.size.get();
        let emission_valid = match p.emission_type {
            EmissionType::Raw => {
                (1..=EMISSION_MAX_INDICES).contains(&size)
                    && (size >= BUFFER_SIZE_MIN as u32 || p.rep.get() == REP_INFINITE)
            }
            EmissionType::Foci => p
                .num_foci
                .zip(p.sound_speed())
                .is_some_and(|(num_foci, _)| {
                    num_foci.get() <= NUM_FOCI_MAX
                        && (BUFFER_SIZE_MIN as u32
                            ..=MAX_FOCI_TOTAL as u32 / u32::from(num_foci.get()))
                            .contains(&size)
                }),
        };
        if !emission_valid {
            return Err(Error::InvalidPayload);
        }
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
