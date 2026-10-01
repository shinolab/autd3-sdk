use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::PhaseCorrPayload;

use crate::fpga::{self, PHASE_CORR_WORDS};
use crate::params::{BRAM_CNT_SELECT_PHASE_CORR, BRAM_SELECT_CONTROLLER};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, _)) = PhaseCorrPayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    for j in 0..PHASE_CORR_WORDS {
        let lo = u16::from(p.data[2 * j]);
        let hi = p.data.get(2 * j + 1).copied().unwrap_or(0) as u16;
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            (u16::from(BRAM_CNT_SELECT_PHASE_CORR) << 8) | j as u16,
            (hi << 8) | lo,
        );
    }
    Ok(())
}
