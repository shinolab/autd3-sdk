use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::GpioInPayload;

use crate::fpga;
use crate::params::{
    ADDR_CTL_FLAG, BRAM_SELECT_CONTROLLER, CTL_FLAG_GPIO_IN_0, CTL_FLAG_GPIO_IN_1,
    CTL_FLAG_GPIO_IN_2, CTL_FLAG_GPIO_IN_3,
};
use crate::port::Port;
use crate::proto::Error;

pub const GPIO_IN_FLAG_MASK: u8 = 0x0F;

const GPIO_IN_MASK: u16 =
    CTL_FLAG_GPIO_IN_0 | CTL_FLAG_GPIO_IN_1 | CTL_FLAG_GPIO_IN_2 | CTL_FLAG_GPIO_IN_3;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, _)) = GpioInPayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    if p.flag > GPIO_IN_FLAG_MASK {
        return Err(Error::InvalidPayload);
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
        if (p.flag & (1u8 << bit)) != 0 {
            ctl |= mask;
        }
    }
    fpga::write(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, ctl);
    Ok(())
}
