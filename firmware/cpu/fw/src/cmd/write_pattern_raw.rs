use zerocopy::FromBytes;

pub use autd3_cpu_wire::layout::{PATTERN_RAW_DATA_LEN, PATTERN_RAW_MAX_COUNT};
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
    let count = usize::from(p.count);
    if usize::from(p.bank) >= NUM_BANKS
        || !(1..=PATTERN_RAW_MAX_COUNT).contains(&count)
        || index + count as u32 > EMISSION_MAX_INDICES
        || rest.len() < count * PATTERN_RAW_DATA_LEN
    {
        return Err(Error::InvalidPayload);
    }
    let (slots, _) = rest.as_chunks::<PATTERN_RAW_DATA_LEN>();
    for (k, data) in slots.iter().take(count).enumerate() {
        let (phases, intensities) = data.split_at(NUM_TRANSDUCERS);
        write_slot(
            port,
            p.bank,
            (index + k as u32) * EMISSION_SLOT_WORDS,
            phases.try_into().map_err(|_| Error::InvalidPayload)?,
            intensities.try_into().map_err(|_| Error::InvalidPayload)?,
        );
    }
    Ok(())
}

pub(crate) fn write_slot<P: Port>(
    port: &mut P,
    bank: u8,
    offset: u32,
    phases: &[u8; NUM_TRANSDUCERS],
    intensities: &[u8; NUM_TRANSDUCERS],
) {
    fpga::write_ram_interleaved(
        port,
        BRAM_SELECT_EMISSION,
        ADDR_PATTERN_MEM_WR_BANK,
        ADDR_PATTERN_MEM_WR_PAGE,
        bank,
        offset,
        phases,
        intensities,
    );
}
