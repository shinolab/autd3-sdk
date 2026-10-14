use crate::fpga;
use crate::fpga_params::{ADDR_FPGA_STATE, FpgaStateFlags};
use crate::port::Port;
use crate::proto::ReplyData;

pub(crate) fn read<P: Port>(port: &mut P) -> ReplyData {
    let mut state = FpgaStateFlags::from_bits_retain(fpga::read_ctl(port, ADDR_FPGA_STATE) as u8);
    if fpga::watchdog_state(port).fired {
        state |= FpgaStateFlags::FAILSAFE;
    }
    ReplyData::from_slice(&[state.bits()])
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::fpga_params::{ADDR_FPGA_STATE, ADDR_FUNCTION_BITS, FunctionBits};
    use crate::proto::{Cmd, Error};
    use crate::test_utils::mock::{Frame, Harness};

    #[test]
    fn read_fpga_state_returns_register_byte() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x83);
        h.deliver(&Frame::new(0, Cmd::ReadFpgaState));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.reply_data(), [0x83]);
    }

    #[test]
    fn a_fired_watchdog_reads_as_the_failsafe_bit() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x2A03);
        h.port.watchdog_state = 0x0101;
        h.deliver(&Frame::new(0, Cmd::ReadFpgaState));
        assert_eq!(h.reply_data(), [0x83]);
    }

    #[test]
    fn an_fpga_without_the_watchdog_reports_the_register_byte_alone() {
        let mut h = Harness::new();
        h.set_ctl(
            ADDR_FUNCTION_BITS,
            u16::from(FunctionBits::STRICT_SILENCER_GUARD.bits()),
        );
        h.set_ctl(ADDR_FPGA_STATE, 0x03);
        h.port.watchdog_state = 0x0101;
        h.deliver(&Frame::new(0, Cmd::ReadFpgaState));
        assert_eq!(h.reply_data(), [0x03]);
    }
}
