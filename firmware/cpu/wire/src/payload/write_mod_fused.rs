use zerocopy::little_endian::{U16, U32, U64};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::config_mod::validate_mod_config;
use super::{TransitionMode, try_read_header};
use crate::PAYLOAD_BYTES;
use crate::{Error, ModulationBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WriteModulationFusedPayload {
    pub bank: ModulationBank,
    pub transition_mode: TransitionMode,
    pub divider: U16,
    pub size: U32,
    pub rep: U16,
    pub transition_value: U64,
    pub margin_ns: U32,
}

impl WriteModulationFusedPayload {
    pub fn parse(payload: &[u8]) -> Result<(Self, &[u8]), Error> {
        let (p, data) = try_read_header::<Self>(payload)?;
        if data.len() > PAYLOAD_BYTES - core::mem::size_of::<Self>() {
            return Err(Error::InvalidPayload);
        }
        validate_mod_config(p.divider.get(), p.size.get())?;
        Ok((p, data))
    }
}

const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, transition_mode) == 1);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, divider) == 2);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, size) == 4);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, rep) == 8);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, transition_value) == 10);
const _: () = assert!(core::mem::offset_of!(WriteModulationFusedPayload, margin_ns) == 18);
const _: () = assert!(core::mem::size_of::<WriteModulationFusedPayload>() == 22);
