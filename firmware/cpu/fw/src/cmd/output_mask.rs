pub use autd3_cpu_wire::payload::OutputMaskPayload;

use crate::fpga;
use crate::params::BRAM_SELECT_OUTPUT_MASK;
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = OutputMaskPayload::parse(payload)?;
    for (j, chunk) in p.data.chunks(16).enumerate() {
        let value = chunk
            .iter()
            .enumerate()
            .filter(|&(_, &on)| on != 0)
            .fold(0u16, |acc, (k, _)| acc | (1 << k));
        fpga::write(port, BRAM_SELECT_OUTPUT_MASK, j as u16, value);
    }
    Ok(())
}
