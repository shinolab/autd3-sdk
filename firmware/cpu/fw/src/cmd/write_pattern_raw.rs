pub use autd3_cpu_wire::payload::WritePatternRawPayload;

use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_TRANSDUCERS,
};
use crate::port::Port;
use crate::proto::{EMISSION_SLOT_WORDS, Error};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, slots) = WritePatternRawPayload::parse(payload)?;
    let index = u32::from(p.index.get());
    for (k, data) in slots.iter().enumerate() {
        let (phases, intensities) = data.split_at(NUM_TRANSDUCERS);
        write_slot(
            port,
            p.bank.as_u8(),
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
