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

pub(super) fn validate_mod_config(divider: u16, size: u32) -> Result<(), Error> {
    if divider == 0 || !(BUFFER_SIZE_MIN as u32..=MOD_BUFFER_SAMPLES as u32).contains(&size) {
        return Err(Error::InvalidPayload);
    }
    Ok(())
}

impl ConfigModPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p = try_read_exact::<Self>(payload)?;
        validate_mod_config(p.divider.get(), p.size.get())?;
        Ok(p)
    }
}

const _: () = assert!(core::mem::offset_of!(ConfigModPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(ConfigModPayload, divider) == 2);
const _: () = assert!(core::mem::offset_of!(ConfigModPayload, size) == 4);
const _: () = assert!(core::mem::offset_of!(ConfigModPayload, rep) == 8);
const _: () = assert!(core::mem::size_of::<ConfigModPayload>() == 10);
