use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

#[derive(
    FromBytes,
    IntoBytes,
    KnownLayout,
    Immutable,
    Unaligned,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    Default,
)]
#[repr(C)]
pub struct Telemetry {
    pub fifo_drop: U32,
    pub exec_late: U32,
    pub sync_resync: u8,
    pub cpu_watchdog: u8,
    pub boot_failure: u8,
}

assert_layout!(Telemetry {
    fifo_drop: 0,
    exec_late: 4,
    sync_resync: 8,
    cpu_watchdog: 9,
    boot_failure: 10,
} size 11);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_telemetry_is_these_bytes_on_the_wire() {
        let telemetry = Telemetry {
            fifo_drop: U32::new(0x0102_0304),
            exec_late: U32::new(0x0506_0708),
            sync_resync: 0x09,
            cpu_watchdog: 0x0A,
            boot_failure: 0x0B,
        };
        assert_eq!(
            telemetry.as_bytes(),
            &[
                0x04, 0x03, 0x02, 0x01, 0x08, 0x07, 0x06, 0x05, 0x09, 0x0A, 0x0B,
            ]
        );
    }
}
