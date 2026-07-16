use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::PwePayload;

use crate::fpga;
use crate::params::BRAM_SELECT_PWE_TABLE;
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, _)) = PwePayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    for (i, value) in p.table.iter().enumerate() {
        fpga::write(port, BRAM_SELECT_PWE_TABLE, i as u16, value.get());
    }
    Ok(())
}
