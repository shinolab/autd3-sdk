pub use autd3_cpu_wire::payload::PhaseCorrPayload;

use crate::fpga::{self, PHASE_CORR_WORDS};
use crate::params::{BRAM_CNT_SELECT_PHASE_CORR, BRAM_SELECT_CONTROLLER};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = PhaseCorrPayload::parse(payload)?;
    for j in 0..PHASE_CORR_WORDS {
        let lo = u16::from(p.data[2 * j]);
        let hi = u16::from(p.data.get(2 * j + 1).copied().unwrap_or(0));
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            (u16::from(BRAM_CNT_SELECT_PHASE_CORR) << 8) | j as u16,
            (hi << 8) | lo,
        );
    }
    Ok(())
}
