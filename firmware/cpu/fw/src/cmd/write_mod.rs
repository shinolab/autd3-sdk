use zerocopy::FromBytes;

pub use autd3_cpu_wire::layout::MOD_WRITE_MAX_DATA_LEN;
pub use autd3_cpu_wire::payload::WriteModPayload;

use crate::fpga;
use crate::params::{ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, BRAM_SELECT_MOD, NUM_BANKS};
use crate::port::Port;
use crate::proto::{Error, MOD_BUFFER_SAMPLES};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, rest)) = WriteModPayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    let offset = p.offset.get();
    let data_len = p.data_len.get();

    if usize::from(p.bank) >= NUM_BANKS
        || !offset.is_multiple_of(2)
        || usize::from(data_len) > MOD_WRITE_MAX_DATA_LEN
        || offset > MOD_BUFFER_SAMPLES
        || u32::from(data_len) > MOD_BUFFER_SAMPLES - offset
    {
        return Err(Error::InvalidPayload);
    }

    fpga::write_ram(
        port,
        BRAM_SELECT_MOD,
        ADDR_MOD_MEM_WR_BANK,
        ADDR_MOD_MEM_WR_PAGE,
        p.bank,
        offset / 2,
        &rest[..usize::from(data_len)],
    );
    Ok(())
}
