use zerocopy::FromBytes;

pub use autd3_cpu_wire::layout::PATTERN_RAW_DATA_LEN;
pub use autd3_cpu_wire::payload::WritePatternRawPayload;

use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, EMISSION_MAX_INDICES,
    NUM_BANKS, NUM_TRANSDUCERS,
};
use crate::port::Port;
use crate::proto::{EMISSION_SLOT_WORDS, Error};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, rest)) = WritePatternRawPayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    let index = u32::from(p.index.get());
    let Some(data) = rest.first_chunk::<PATTERN_RAW_DATA_LEN>() else {
        return Err(Error::InvalidPayload);
    };
    if usize::from(p.bank) >= NUM_BANKS || index >= EMISSION_MAX_INDICES {
        return Err(Error::InvalidPayload);
    }
    write_slot(port, p.bank, index * EMISSION_SLOT_WORDS, data);
    Ok(())
}

pub(crate) fn write_slot<P: Port>(
    port: &mut P,
    bank: u8,
    offset: u32,
    data: &[u8; PATTERN_RAW_DATA_LEN],
) {
    let (phases, intensities) = data.split_at(NUM_TRANSDUCERS);
    fpga::write_ram_interleaved(
        port,
        BRAM_SELECT_EMISSION,
        ADDR_PATTERN_MEM_WR_BANK,
        ADDR_PATTERN_MEM_WR_PAGE,
        bank,
        offset,
        phases.try_into().expect("half of PATTERN_RAW_DATA_LEN"),
        intensities
            .try_into()
            .expect("half of PATTERN_RAW_DATA_LEN"),
    );
}
