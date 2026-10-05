use core::num::NonZeroU16;

use zerocopy::little_endian::{U16, U32};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_exact;
use crate::layout::{BUFFER_SIZE_MIN, MOD_BUFFER_SAMPLES};
use crate::{Error, ModulationBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigModPayload {
    pub bank: ModulationBank,
    pub reserved: u8,
    pub divider: U16,
    pub size: U32,
    pub rep: U16,
}

impl ConfigModPayload {
    #[must_use]
    pub const fn divider(&self) -> Option<NonZeroU16> {
        NonZeroU16::new(self.divider.get())
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p = try_read_exact::<Self>(payload)?;
        p.divider().ok_or(Error::InvalidPayload)?;
        if !(BUFFER_SIZE_MIN as u32..=MOD_BUFFER_SAMPLES as u32).contains(&p.size.get()) {
            return Err(Error::InvalidPayload);
        }
        Ok(p)
    }
}

const _: () = assert!(core::mem::offset_of!(ConfigModPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(ConfigModPayload, divider) == 2);
const _: () = assert!(core::mem::offset_of!(ConfigModPayload, size) == 4);
const _: () = assert!(core::mem::offset_of!(ConfigModPayload, rep) == 8);
const _: () = assert!(core::mem::size_of::<ConfigModPayload>() == 10);
