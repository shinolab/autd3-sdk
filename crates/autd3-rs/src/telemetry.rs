#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Telemetry {
    pub fifo_drop: u32,
    pub exec_late: u32,
    pub sync_resync: u8,
    pub cpu_watchdog: u8,
    pub boot_failure: u8,
}

impl Telemetry {
    pub(crate) fn from_wire(
        autd3_cpu_wire::payload::Telemetry {
            fifo_drop,
            exec_late,
            sync_resync,
            cpu_watchdog,
            boot_failure,
        }: autd3_cpu_wire::payload::Telemetry,
    ) -> Self {
        Self {
            fifo_drop: fifo_drop.get(),
            exec_late: exec_late.get(),
            sync_resync,
            cpu_watchdog,
            boot_failure,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_wire_reads_every_field() {
        let wire = autd3_cpu_wire::payload::Telemetry {
            fifo_drop: 0x0102_0304.into(),
            exec_late: 0x0506_0708.into(),
            sync_resync: 9,
            cpu_watchdog: 10,
            boot_failure: 11,
        };
        assert_eq!(
            Telemetry::from_wire(wire),
            Telemetry {
                fifo_drop: 0x0102_0304,
                exec_late: 0x0506_0708,
                sync_resync: 9,
                cpu_watchdog: 10,
                boot_failure: 11,
            }
        );
    }
}
