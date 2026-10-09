use crate::fpga;
use crate::fpga_params::ADDR_FPGA_STATE;
use crate::port::Port;
use crate::proto::ReplyData;

pub(crate) fn read<P: Port>(port: &mut P) -> ReplyData {
    ReplyData::from_slice(&[fpga::read_ctl(port, ADDR_FPGA_STATE) as u8])
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::fpga_params::ADDR_FPGA_STATE;
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
}
