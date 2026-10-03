use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_header;
use crate::Error;

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ForceFanPayload {
    pub value: bool,
}

impl ForceFanPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_header(payload).map(|(p, _)| p)
    }
}

const _: () = assert!(core::mem::size_of::<ForceFanPayload>() == 1);
