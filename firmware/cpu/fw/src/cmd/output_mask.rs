use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::OutputMaskPayload;

use crate::fpga;
use crate::params::{BRAM_CNT_SELECT_OUTPUT_MASK, BRAM_SELECT_CONTROLLER};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, _)) = OutputMaskPayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    for (j, value) in p.data.iter().enumerate() {
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            (u16::from(BRAM_CNT_SELECT_OUTPUT_MASK) << 8) | j as u16,
            value.get(),
        );
    }
    Ok(())
}
