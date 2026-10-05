use core::num::NonZeroU16;

use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::fpga_params::SilencerFlags;

pub const SILENCER_DEFAULT_UPDATE_RATE: u16 = 256;
pub const SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY: u16 = 10;
pub const SILENCER_DEFAULT_COMPLETION_STEPS_PHASE: u16 = 40;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct SilencerPayload {
    pub flag: u8,
    pub reserved: u8,
    pub update_rate_intensity: U16,
    pub update_rate_phase: U16,
    pub completion_steps_intensity: U16,
    pub completion_steps_phase: U16,
}

impl SilencerPayload {
    #[must_use]
    pub const fn flags(&self) -> SilencerFlags {
        SilencerFlags::from_bits_retain(self.flag)
    }

    #[must_use]
    pub fn update_rate(&self) -> Option<(NonZeroU16, NonZeroU16)> {
        NonZeroU16::new(self.update_rate_intensity.get())
            .zip(NonZeroU16::new(self.update_rate_phase.get()))
    }

    #[must_use]
    pub fn completion_steps(&self) -> Option<(NonZeroU16, NonZeroU16)> {
        NonZeroU16::new(self.completion_steps_intensity.get())
            .zip(NonZeroU16::new(self.completion_steps_phase.get()))
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p = try_read_exact::<Self>(payload)?;
        let active = if p.flags().contains(SilencerFlags::FIXED_UPDATE_RATE_MODE) {
            p.update_rate()
        } else {
            p.completion_steps()
        };
        active.ok_or(Error::InvalidPayload)?;
        Ok(p)
    }
}

const _: () = assert!(core::mem::offset_of!(SilencerPayload, flag) == 0);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, update_rate_intensity) == 2);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, update_rate_phase) == 4);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, completion_steps_intensity) == 6);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, completion_steps_phase) == 8);
const _: () = assert!(core::mem::size_of::<SilencerPayload>() == 10);
