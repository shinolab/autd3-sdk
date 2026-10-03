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
    let p = GpioInPayload::parse(payload)?;
    let mut ctl = fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG) & !GPIO_IN_MASK;
    for (on, mask) in [p.gpio_in_0, p.gpio_in_1, p.gpio_in_2, p.gpio_in_3]
        .into_iter()
        .zip([
            CTL_FLAG_GPIO_IN_0,
            CTL_FLAG_GPIO_IN_1,
            CTL_FLAG_GPIO_IN_2,
            CTL_FLAG_GPIO_IN_3,
        ])
    {
        if on {
            ctl |= mask;
        }
    }
    fpga::write(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, ctl);
    Ok(())
}
