pub use autd3_cpu_wire::payload::WriteFociPayload;

use crate::fpga;
use crate::params::{ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, data) = WriteFociPayload::parse(payload)?;
    fpga::write_ram(
        port,
        BRAM_SELECT_EMISSION,
        ADDR_PATTERN_MEM_WR_BANK,
        ADDR_PATTERN_MEM_WR_PAGE,
        p.bank.as_u8(),
        p.offset.get(),
        data,
    );
    Ok(())
}
