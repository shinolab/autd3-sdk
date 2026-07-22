use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

pub const SILENCER_FLAG_BIT_STRICT_MODE: u8 = 1;
pub const SILENCER_FLAG_STRICT_MODE: u8 = 1 << SILENCER_FLAG_BIT_STRICT_MODE;

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

const _: () = assert!(core::mem::offset_of!(SilencerPayload, flag) == 0);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, update_rate_intensity) == 2);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, update_rate_phase) == 4);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, completion_steps_intensity) == 6);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, completion_steps_phase) == 8);
const _: () = assert!(core::mem::size_of::<SilencerPayload>() == 10);
