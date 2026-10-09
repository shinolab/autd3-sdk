pub use autd3_cpu_wire::payload::GpioInPayload;

use crate::fpga;
use crate::fpga_params::{ADDR_CTL_FLAG, CtlFlags};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = GpioInPayload::parse(payload)?;
    let mut ctl = CtlFlags::from_bits_retain(fpga::read_ctl(port, ADDR_CTL_FLAG));
    ctl.set(CtlFlags::GPIO_IN_0, p.gpio_in_0);
    ctl.set(CtlFlags::GPIO_IN_1, p.gpio_in_1);
    ctl.set(CtlFlags::GPIO_IN_2, p.gpio_in_2);
    ctl.set(CtlFlags::GPIO_IN_3, p.gpio_in_3);
    fpga::write_ctl(port, ADDR_CTL_FLAG, ctl.bits());
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
        h.deliver(&gpio_in(0, [0, 1, 0, 1]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl_flags(), CtlFlags::GPIO_IN_1 | CtlFlags::GPIO_IN_3);
    }

    #[test]
    fn emulate_gpio_in_rejects_out_of_range() {
        let mut h = Harness::new();
        h.deliver(&gpio_in(0, [0, 0, 2, 0]));
        assert_eq!(h.status(), Error::InvalidPayload);
    }
}
