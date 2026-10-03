pub use autd3_cpu_wire::payload::WritePatternCompressedPayload;

use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, NUM_TRANSDUCERS,
};
use crate::port::Port;
use crate::proto::{EMISSION_SLOT_WORDS, Error};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, words) = WritePatternCompressedPayload::parse(payload)?;
    let per_word = p.format.patterns_per_word();
    let mut slot = [0u8; 2 * NUM_TRANSDUCERS];
    for g in 0..p.count {
        let group = &words[usize::from(g / per_word) * NUM_TRANSDUCERS..][..NUM_TRANSDUCERS];
        let sub = g % per_word;
        for (t, word) in group.iter().enumerate() {
            slot[2 * t] = p.format.phase(word.get(), sub);
            slot[2 * t + 1] = p.intensity;
        }
        fpga::write_ram(
            port,
            BRAM_SELECT_EMISSION,
            ADDR_PATTERN_MEM_WR_BANK,
            ADDR_PATTERN_MEM_WR_PAGE,
            p.bank.as_u8(),
            p.offset.get() + u32::from(g) * EMISSION_SLOT_WORDS,
            &slot,
        );
    }
    Ok(())
}
