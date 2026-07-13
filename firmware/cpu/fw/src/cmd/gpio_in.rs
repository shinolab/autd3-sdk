use crate::fpga;
use crate::params::{
    ADDR_CTL_FLAG, BRAM_SELECT_CONTROLLER, CTL_FLAG_GPIO_IN_0, CTL_FLAG_GPIO_IN_1,
    CTL_FLAG_GPIO_IN_2, CTL_FLAG_GPIO_IN_3,
};
use crate::port::Port;
use crate::proto::{ERR_INVALID_PAYLOAD, ERR_NONE, GPIO_IN_FLAG_MASK, GPIO_IN_OFFSET_FLAG};

const GPIO_IN_MASK: u16 =
    CTL_FLAG_GPIO_IN_0 | CTL_FLAG_GPIO_IN_1 | CTL_FLAG_GPIO_IN_2 | CTL_FLAG_GPIO_IN_3;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let flag = payload[GPIO_IN_OFFSET_FLAG];
    if flag > GPIO_IN_FLAG_MASK {
        return ERR_INVALID_PAYLOAD;
    }
    let mut ctl = fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG) & !GPIO_IN_MASK;
    for (bit, mask) in [
        CTL_FLAG_GPIO_IN_0,
        CTL_FLAG_GPIO_IN_1,
        CTL_FLAG_GPIO_IN_2,
        CTL_FLAG_GPIO_IN_3,
    ]
    .into_iter()
    .enumerate()
    {
        if (flag & (1u8 << bit)) != 0 {
            ctl |= mask;
        }
    }
    fpga::write(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, ctl);
    ERR_NONE
}
