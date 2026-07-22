use zerocopy::FromBytes;

pub use autd3_cpu_wire::layout::PATTERN_WRITE_MAX_DATA_LEN;
pub use autd3_cpu_wire::payload::WritePatternPayload;

use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{EMISSION_RAM_WORDS, Error};

const _: () = assert!(crate::params::NUM_TRANSDUCERS * 2 <= PATTERN_WRITE_MAX_DATA_LEN);

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, rest)) = WritePatternPayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    let offset = p.offset.get();
    let data_len = p.data_len.get();

    if usize::from(p.bank) >= NUM_BANKS
        || !data_len.is_multiple_of(2)
        || usize::from(data_len) > PATTERN_WRITE_MAX_DATA_LEN
        || offset > EMISSION_RAM_WORDS
        || u32::from(data_len / 2) > EMISSION_RAM_WORDS - offset
    {
        return Err(Error::InvalidPayload);
    }

    fpga::write_ram(
        port,
        BRAM_SELECT_EMISSION,
        ADDR_PATTERN_MEM_WR_BANK,
        ADDR_PATTERN_MEM_WR_PAGE,
        p.bank,
        offset,
        &rest[..usize::from(data_len)],
    );
    Ok(())
}
