use zerocopy::IntoBytes;

use autd3_cpu_wire::payload::Telemetry;

use crate::app::{Counter, Cpu};
use crate::fpga;
use crate::fpga_params::ADDR_FPGA_STATE;
use crate::port::Port;
use crate::proto::ReplyData;

fn saturate_u16(count: u32) -> u16 {
    u16::try_from(count).unwrap_or(u16::MAX)
}

fn saturate_u8(count: u32) -> u8 {
    u8::try_from(count).unwrap_or(u8::MAX)
}

impl Cpu {
    pub(crate) fn read_telemetry<P: Port>(&self, port: &mut P) -> ReplyData {
        let telemetry = Telemetry {
            fifo_drop: self.counter(Counter::FifoDrop).into(),
            dedup: self.counter(Counter::Dedup).into(),
            seq_mismatch: self.counter(Counter::SeqMismatch).into(),
            dispatch_error: self.counter(Counter::DispatchError).into(),
            processed: self.counter(Counter::Processed).into(),
            send_failure: self.counter(Counter::SendFailure).into(),
            failsafe: saturate_u16(self.counter(Counter::Failsafe)).into(),
            ptp_unlock_failsafe: saturate_u16(self.counter(Counter::PtpUnlockFailsafe)).into(),
            sync_resync: (fpga::read_ctl(port, ADDR_FPGA_STATE) >> 8) as u8,
            boot_failure: saturate_u8(self.counter(Counter::BootFailure)),
        };
        ReplyData::from_slice(telemetry.as_bytes())
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use autd3_cpu_wire::payload::Telemetry;

    use crate::app::Counter;
    use crate::fpga_params::ADDR_FPGA_STATE;
    use crate::proto::{Cmd, Error};
    use crate::test_utils::builders::force_fan;
    use crate::test_utils::mock::{Frame, Harness};

    use super::{saturate_u8, saturate_u16};

    fn read_telemetry(h: &mut Harness, seq: u8) -> Telemetry {
        use zerocopy::FromBytes;
        h.deliver(&Frame::new(seq, Cmd::ReadTelemetry));
        assert_eq!(h.status(), Error::None);
        Telemetry::read_from_bytes(&h.reply_data()[..]).expect("a telemetry reply")
    }

    #[test]
    fn read_telemetry_returns_every_counter_at_once() {
        let mut h = Harness::new();
        h.deliver(&force_fan(0, 2));

        let telemetry = read_telemetry(&mut h, 1);
        assert_eq!(telemetry.dispatch_error.get(), 1);
        assert_eq!(telemetry.processed.get(), 1);
        assert_eq!(telemetry.fifo_drop.get(), 0);
    }

    #[test]
    fn read_telemetry_sync_resync_returns_fpga_state_high_byte() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x2A83);
        assert_eq!(read_telemetry(&mut h, 0).sync_resync, 0x2A);
    }

    #[test]
    fn frame_counters_are_wider_than_a_byte() {
        let mut h = Harness::new();
        let mut seq = 0u8;
        for _ in 0..300 {
            h.deliver(&Frame::new(seq, Cmd::Nop));
            seq = seq.wrapping_add(1);
        }
        assert_eq!(read_telemetry(&mut h, seq).processed.get(), 300);
    }

    #[test]
    fn read_telemetry_reports_boot_failures() {
        let mut h = Harness::new();
        h.cpu.count_boot_failure();
        assert_eq!(h.counter(Counter::BootFailure), 1);
        assert_eq!(read_telemetry(&mut h, 0).boot_failure, 1);
    }

    #[test]
    fn narrowed_fields_saturate_instead_of_wrapping() {
        assert_eq!(saturate_u16(u32::from(u16::MAX)), u16::MAX);
        assert_eq!(saturate_u16(u32::from(u16::MAX) + 1), u16::MAX);
        assert_eq!(saturate_u16(u32::MAX), u16::MAX);
        assert_eq!(saturate_u8(u32::from(u8::MAX) + 1), u8::MAX);
        assert_eq!(saturate_u8(7), 7);
    }
}
