use crate::fpga;
use crate::fpga_params::{ADDR_FPGA_STATE, FpgaStateFlags};
use crate::port::Port;
use crate::proto::ReplyData;

pub(crate) fn read<P: Port>(port: &mut P) -> ReplyData {
    let state = FpgaStateFlags::from_bits_retain(fpga::read_ctl(port, ADDR_FPGA_STATE) as u8)
        - FpgaStateFlags::FAILSAFE;
    ReplyData::from_slice(&[state.bits()])
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::fpga_params::ADDR_FPGA_STATE;
    use crate::proto::{Cmd, Error};
    use crate::test_utils::mock::{Frame, Harness};

    #[test]
    fn read_fpga_state_returns_register_byte() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x2A7F);
        h.deliver(&Frame::new(Cmd::ReadFpgaState));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.reply_data(), [0x7F]);
    }

    #[test]
    fn the_failsafe_bit_is_never_reported() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FPGA_STATE, 0x83);
        h.port.watchdog_state = 0x0101;
        h.deliver(&Frame::new(Cmd::ReadFpgaState));
        assert_eq!(h.reply_data(), [0x03]);
    }
}
