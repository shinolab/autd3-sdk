use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::frame::REPLY_DATA_BYTES_MAX;

#[derive(
    FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug,
)]
#[repr(C)]
pub struct Telemetry {
    pub fifo_drop: U32,
    pub dispatch_error: U32,
    pub processed: U32,
    pub send_failure: U32,
    pub failsafe: U16,
    pub ptp_unlock_failsafe: U16,
    pub sync_resync: u8,
    pub boot_failure: u8,
    pub cpu_watchdog: u8,
    pub exec_late: U32,
}

const _: () = assert!(core::mem::size_of::<Telemetry>() == 27);
const _: () = assert!(core::mem::size_of::<Telemetry>() <= REPLY_DATA_BYTES_MAX);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_telemetry_is_these_bytes_on_the_wire() {
        let telemetry = Telemetry {
            fifo_drop: U32::new(0x0102_0304),
            dispatch_error: U32::new(0x0506_0708),
            processed: U32::new(0x090A_0B0C),
            send_failure: U32::new(0x0D0E_0F10),
            failsafe: U16::new(0x1112),
            ptp_unlock_failsafe: U16::new(0x1314),
            sync_resync: 0x15,
            boot_failure: 0x16,
            cpu_watchdog: 0x17,
            exec_late: U32::new(0x1B1A_1918),
        };
        assert_eq!(
            telemetry.as_bytes(),
            &[
                0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05, 0x0C, 0x0B, 0x0A, 0x09, 0x10, 0x0F,
                0x0E, 0x0D, 0x12, 0x11, 0x14, 0x13, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B,
            ]
        );
    }
}
