pub use autd3_cpu_wire::payload::WritePatternPhasePayload;

use super::write_pattern_raw::write_slot;
use crate::params::NUM_TRANSDUCERS;
use crate::port::Port;
use crate::proto::{EMISSION_SLOT_WORDS, Error};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, data) = WritePatternPhasePayload::parse(payload)?;
    let index = u32::from(p.index.get());
    let intensities = [p.intensity; NUM_TRANSDUCERS];
    let mut phases = [0u8; NUM_TRANSDUCERS];
    for (g, pattern) in data.chunks_exact(p.depth.bytes_per_pattern()).enumerate() {
        for (t, phase) in phases.iter_mut().enumerate() {
            *phase = p.depth.phase(pattern, t);
        }
        write_slot(
            port,
            p.bank.as_u8(),
            (index + g as u32) * EMISSION_SLOT_WORDS,
            &phases,
            &intensities,
        );
    }
    Ok(())
}
