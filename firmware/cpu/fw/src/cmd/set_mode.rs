use core::mem::offset_of;

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::app::Cpu;
use crate::proto::{Error, Mode};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct SetModePayload {
    pub mode: u8,
}

const _: () = assert!(offset_of!(SetModePayload, mode) == 0);

impl Cpu {
    pub(crate) fn set_mode_cmd(&self, payload: &[u8]) -> Result<(), Error> {
        let Ok((p, _)) = SetModePayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let Some(mode) = Mode::from_u8(p.mode) else {
            return Err(Error::InvalidPayload);
        };
        self.set_mode(mode);
        Ok(())
    }
}
