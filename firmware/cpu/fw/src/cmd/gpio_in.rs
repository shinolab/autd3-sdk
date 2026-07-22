use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::GpioInPayload;

use crate::fpga;
use crate::params::{
    ADDR_CTL_FLAG, BRAM_SELECT_CONTROLLER, CTL_FLAG_GPIO_IN_0, CTL_FLAG_GPIO_IN_1,
    CTL_FLAG_GPIO_IN_2, CTL_FLAG_GPIO_IN_3,
};
use crate::port::Port;
use crate::proto::Error;

const GPIO_IN_MASK: u16 =
    CTL_FLAG_GPIO_IN_0 | CTL_FLAG_GPIO_IN_1 | CTL_FLAG_GPIO_IN_2 | CTL_FLAG_GPIO_IN_3;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, _)) = GpioInPayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    let values = [p.gpio_in_0, p.gpio_in_1, p.gpio_in_2, p.gpio_in_3];
    if values.iter().any(|&v| v > 1) {
        return Err(Error::InvalidPayload);
    }
    let mut ctl = fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG) & !GPIO_IN_MASK;
    for (value, mask) in values.into_iter().zip([
        CTL_FLAG_GPIO_IN_0,
        CTL_FLAG_GPIO_IN_1,
        CTL_FLAG_GPIO_IN_2,
        CTL_FLAG_GPIO_IN_3,
    ]) {
        if value != 0 {
            ctl |= mask;
        }
    }
    fpga::write(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, ctl);
    Ok(())
}
