pub use autd3_cpu_wire::payload::GpioOutPayload;

use crate::app::Cpu;
use crate::fpga;
use crate::params::{ADDR_DEBUG_VALUE0_0, CTL_FLAG_DEBUG_SET};
use crate::port::Port;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn gpio_out<P: Port>(&self, port: &mut P, payload: &[u8]) -> Result<(), Error> {
        let p = GpioOutPayload::parse(payload)?;
        for (i, value) in p.values.iter().enumerate() {
            fpga::write_u64(port, ADDR_DEBUG_VALUE0_0 + 4 * i as u16, value.get());
        }
        self.set_and_wait_update(port, CTL_FLAG_DEBUG_SET)
    }
}
