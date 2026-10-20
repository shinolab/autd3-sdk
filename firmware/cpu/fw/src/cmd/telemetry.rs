use zerocopy::IntoBytes;

use crate::app::Cpu;
use crate::fpga;
use crate::fpga_params::ADDR_FPGA_STATE;
use crate::port::Port;
use crate::proto::ReplyData;

impl Cpu {
    pub(crate) fn read_telemetry<P: Port>(&self, port: &mut P) -> ReplyData {
        let sync_resync = (fpga::read_ctl(port, ADDR_FPGA_STATE) >> 8) as u8;
        let cpu_watchdog = fpga::watchdog_state(port).count;
        let telemetry = self.telemetry(sync_resync, cpu_watchdog);
        ReplyData::from_slice(telemetry.as_bytes())
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use autd3_cpu_wire::payload::Telemetry;

    use crate::fpga_params::{ADDR_FPGA_STATE, ADDR_FUNCTION_BITS, FunctionBits};
    use crate::proto::{Cmd, Error};
    use crate::test_utils::builders::force_fan;
    use crate::test_utils::mock::{Frame, Harness};

    fn read_telemetry(h: &mut Harness) -> Telemetry {
        use zerocopy::FromBytes;
        h.deliver(&Frame::new(Cmd::ReadTelemetry));
        assert_eq!(h.status(), Error::None);
        Telemetry::read_from_bytes(&h.reply_data()[..]).expect("a telemetry reply")
    }

    #[test]
    fn read_telemetry_returns_every_counter_at_once() {
        let mut h = Harness::new();
        h.deliver(&force_fan(2));

        let telemetry = read_telemetry(&mut h);
        assert_eq!(telemetry.dispatch_error.get(), 1);
        assert_eq!(telemetry.processed.get(), 1);
        assert_eq!(telemetry.fifo_drop.get(), 0);
    }

    #[test]
    fn read_telemetry_sync_resync_returns_fpga_state_high_byte() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x2A83);
        assert_eq!(read_telemetry(&mut h).sync_resync, 0x2A);
    }

    #[test]
    fn read_telemetry_cpu_watchdog_returns_the_fpga_fire_count() {
        let mut h = Harness::new();
        h.port.watchdog_state = 0x2A01;
        assert_eq!(read_telemetry(&mut h).cpu_watchdog, 0x2A);

        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(read_telemetry(&mut h).cpu_watchdog, 0x2A);
    }

    #[test]
    fn an_fpga_without_the_watchdog_reports_no_fires() {
        let mut h = Harness::new();
        h.set_ctl(
            ADDR_FUNCTION_BITS,
            u16::from(FunctionBits::STRICT_SILENCER_GUARD.bits()),
        );
        h.port.watchdog_state = 0x2A01;
        assert_eq!(read_telemetry(&mut h).cpu_watchdog, 0);
    }

    #[test]
    fn frame_counters_are_wider_than_a_byte() {
        let mut h = Harness::new();
        for _ in 0..300 {
            h.deliver(&Frame::new(Cmd::Nop));
        }
        assert_eq!(read_telemetry(&mut h).processed.get(), 300);
    }

    #[test]
    fn read_telemetry_reports_boot_failures() {
        let mut h = Harness::new();
        h.cpu.count_boot_failure();
        assert_eq!(h.telemetry().boot_failure, 1);
        assert_eq!(read_telemetry(&mut h).boot_failure, 1);
    }

    #[test]
    fn boot_failures_saturate_instead_of_wrapping() {
        let mut h = Harness::new();
        for _ in 0..=u32::from(u8::MAX) {
            h.cpu.count_boot_failure();
        }
        assert_eq!(read_telemetry(&mut h).boot_failure, u8::MAX);
    }
}
