use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::frame::REPLY_DATA_BYTES_MAX;

#[derive(
    FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug,
)]
#[repr(C)]
pub struct Telemetry {
    pub fifo_drop: U32,
    pub dedup: U32,
    pub seq_mismatch: U32,
    pub dispatch_error: U32,
    pub processed: U32,
    pub send_failure: U32,
    pub failsafe: U16,
    pub ptp_unlock_failsafe: U16,
    pub sync_resync: u8,
    pub boot_failure: u8,
}

const _: () = assert!(core::mem::size_of::<Telemetry>() == 30);
const _: () = assert!(core::mem::size_of::<Telemetry>() <= REPLY_DATA_BYTES_MAX);
