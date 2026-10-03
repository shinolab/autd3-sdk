pub use autd3_cpu_wire::payload::WriteModPayload;

use crate::fpga;
use crate::params::{ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, BRAM_SELECT_MOD};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, data) = WriteModPayload::parse(payload)?;
    fpga::write_ram(
        port,
        BRAM_SELECT_MOD,
        ADDR_MOD_MEM_WR_BANK,
        ADDR_MOD_MEM_WR_PAGE,
        p.bank.as_u8(),
        p.offset.get() / 2,
        data,
    );
    Ok(())
}
