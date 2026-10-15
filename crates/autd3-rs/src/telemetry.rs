use zerocopy::FromBytes;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Telemetry {
    pub fifo_drop: u32,
    pub dedup: u32,
    pub seq_mismatch: u32,
    pub dispatch_error: u32,
    pub processed: u32,
    pub send_failure: u32,
    pub failsafe: u16,
    pub ptp_unlock_failsafe: u16,
    pub sync_resync: u8,
    pub boot_failure: u8,
    pub cpu_watchdog: u8,
    pub exec_late: u32,
}

impl Telemetry {
    pub(crate) fn parse(value: &[u8]) -> Option<Self> {
        let (wire, _) = autd3_cpu_wire::payload::Telemetry::read_from_prefix(value).ok()?;
        Some(Self {
            fifo_drop: wire.fifo_drop.get(),
            dedup: wire.dedup.get(),
            seq_mismatch: wire.seq_mismatch.get(),
            dispatch_error: wire.dispatch_error.get(),
            processed: wire.processed.get(),
            send_failure: wire.send_failure.get(),
            failsafe: wire.failsafe.get(),
            ptp_unlock_failsafe: wire.ptp_unlock_failsafe.get(),
            sync_resync: wire.sync_resync,
            boot_failure: wire.boot_failure,
            cpu_watchdog: wire.cpu_watchdog,
            exec_late: wire.exec_late.get(),
        })
    }
}

#[cfg(test)]
mod tests {
    use zerocopy::IntoBytes;

    use super::*;

    fn wire() -> autd3_cpu_wire::payload::Telemetry {
        autd3_cpu_wire::payload::Telemetry {
            fifo_drop: 1.into(),
            dedup: 2.into(),
            seq_mismatch: 3.into(),
            dispatch_error: 4.into(),
            processed: 0x1234_5678.into(),
            send_failure: 6.into(),
            failsafe: 0x0708.into(),
            ptp_unlock_failsafe: 9.into(),
            sync_resync: 10,
            boot_failure: 11,
            cpu_watchdog: 12,
            exec_late: 0x0D0E_0F10.into(),
        }
    }

    #[test]
    fn parse_reads_every_field() {
        let telemetry = Telemetry::parse(wire().as_bytes()).unwrap();
        assert_eq!(
            telemetry,
            Telemetry {
                fifo_drop: 1,
                dedup: 2,
                seq_mismatch: 3,
                dispatch_error: 4,
                processed: 0x1234_5678,
                send_failure: 6,
                failsafe: 0x0708,
                ptp_unlock_failsafe: 9,
                sync_resync: 10,
                boot_failure: 11,
                cpu_watchdog: 12,
                exec_late: 0x0D0E_0F10,
            }
        );
    }

    #[test]
    fn a_longer_reply_is_read_from_its_prefix() {
        let mut bytes = wire().as_bytes().to_vec();
        bytes.extend_from_slice(&[0xFF; 4]);
        assert_eq!(
            Telemetry::parse(&bytes),
            Telemetry::parse(wire().as_bytes())
        );
    }

    #[test]
    fn a_short_reply_is_rejected() {
        assert!(Telemetry::parse(&[0; 34]).is_none());
    }
}
