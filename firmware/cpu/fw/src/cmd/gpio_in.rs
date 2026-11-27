pub(crate) use autd3_cpu_wire::payload::GpioInPayload;

use crate::fpga;
use crate::fpga_params::{ADDR_CTL_FLAG, CtlFlags};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = GpioInPayload::parse(payload)?;
    let mut ctl = fpga::controller(port);
    let mut flags = CtlFlags::from_bits_retain(ctl.read(ADDR_CTL_FLAG));
    for (pin, value) in [
        CtlFlags::GPIO_IN_0,
        CtlFlags::GPIO_IN_1,
        CtlFlags::GPIO_IN_2,
        CtlFlags::GPIO_IN_3,
    ]
    .into_iter()
    .zip(p.values)
    {
        flags.set(pin, value);
    }
    ctl.write(ADDR_CTL_FLAG, flags.bits());
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::fpga_params::CtlFlags;
    use crate::proto::Error;
    use crate::test_utils::builders::gpio_in;
    use crate::test_utils::mock::Harness;

    #[test]
    fn emulate_gpio_in_maps_values() {
        let mut h = Harness::new();
        h.deliver(&gpio_in([0, 1, 0, 1]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl_flags(), CtlFlags::GPIO_IN_1 | CtlFlags::GPIO_IN_3);
    }

    #[test]
    fn emulate_gpio_in_rejects_out_of_range() {
        let mut h = Harness::new();
        h.deliver(&gpio_in([0, 0, 2, 0]));
        assert_eq!(h.status(), Error::InvalidPayload);
    }
}
