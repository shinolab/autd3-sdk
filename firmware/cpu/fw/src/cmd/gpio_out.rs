pub use autd3_cpu_wire::payload::GpioOutPayload;

use core::num::NonZeroU32;

use crate::fpga;
use crate::fpga_params::{ADDR_DEBUG_VALUE0_0, CtlFlags};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(
    port: &mut P,
    payload: &[u8],
    max_polls: NonZeroU32,
) -> Result<(), Error> {
    let p = GpioOutPayload::parse(payload)?;
    for (i, value) in p.values.iter().enumerate() {
        fpga::write_u64(port, ADDR_DEBUG_VALUE0_0 + 4 * i as u16, value.get());
    }
    fpga::set_and_wait_update(port, CtlFlags::DEBUG_SET, max_polls)
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec;
    use std::vec::Vec;

    use crate::fpga_params::{ADDR_DEBUG_VALUE0_0, CtlFlags};
    use crate::proto::Error;
    use crate::test_utils::builders::gpio_out;
    use crate::test_utils::mock::Harness;

    #[test]
    fn gpio_out_writes_debug_values_and_latches() {
        let mut h = Harness::new();
        let latches_at_boot = h.latch_count(CtlFlags::DEBUG_SET);
        let values: Vec<u64> = vec![
            0x0102_0304_0506_0708,
            0x1112_1314_1516_1718,
            0x2122_2324_2526_2728,
            0x3132_3334_3536_3738,
        ];
        h.deliver(&gpio_out(0, &values));
        assert_eq!(h.status(), Error::None);
        for (v, value) in values.iter().enumerate() {
            for w in 0..4u32 {
                let expect = ((value >> (16 * w)) & 0xFFFF) as u16;
                let addr = ADDR_DEBUG_VALUE0_0 + (v as u16) * 4 + (w as u16);
                assert_eq!(h.ctl(addr), expect);
            }
        }
        assert_eq!(h.latch_count(CtlFlags::DEBUG_SET), latches_at_boot + 1);
    }
}
