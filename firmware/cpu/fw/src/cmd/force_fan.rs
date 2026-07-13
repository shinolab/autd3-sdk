use crate::fpga;
use crate::params::{ADDR_CTL_FLAG, BRAM_SELECT_CONTROLLER, CTL_FLAG_FORCE_FAN};
use crate::port::Port;
use crate::proto::{ERR_INVALID_PAYLOAD, ERR_NONE, FORCE_FAN_OFFSET_VALUE};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let value = payload[FORCE_FAN_OFFSET_VALUE];
    if value > 1 {
        return ERR_INVALID_PAYLOAD;
    }
    let mut ctl = fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG);
    if value == 0 {
        ctl &= !CTL_FLAG_FORCE_FAN;
    } else {
        ctl |= CTL_FLAG_FORCE_FAN;
    }
    fpga::write(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, ctl);
    ERR_NONE
}
