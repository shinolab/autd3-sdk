pub use autd3_cpu_wire::payload::ForceFanPayload;

use crate::fpga;
use crate::params::{ADDR_CTL_FLAG, BRAM_SELECT_CONTROLLER, CTL_FLAG_FORCE_FAN};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = ForceFanPayload::parse(payload)?;
    let mut ctl = fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG);
    if p.value {
        ctl |= CTL_FLAG_FORCE_FAN;
    } else {
        ctl &= !CTL_FLAG_FORCE_FAN;
    }
    fpga::write(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, ctl);
    Ok(())
}
