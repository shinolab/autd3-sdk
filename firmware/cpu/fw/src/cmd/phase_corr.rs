pub use autd3_cpu_wire::payload::PhaseCorrPayload;

use crate::fpga;
use crate::fpga_params::BramSelect;
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = PhaseCorrPayload::parse(payload)?;
    for (j, word) in fpga::le_words(&p.data, 0).enumerate() {
        fpga::write(port, BramSelect::PhaseCorr, j as u16, word);
    }
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec::Vec;

    use crate::fpga_params::NUM_TRANSDUCERS;
    use crate::proto::Error;
    use crate::test_utils::builders::phase_corr;
    use crate::test_utils::mock::Harness;

    #[test]
    fn phase_corr_packs_bytes_into_words() {
        let mut h = Harness::new();
        let phases: Vec<u8> = (0..NUM_TRANSDUCERS).map(|i| (i & 0xFF) as u8).collect();
        h.deliver(&phase_corr(0, &phases));
        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.port.phase_corr[0],
            (u16::from(phases[1]) << 8) | u16::from(phases[0])
        );
        assert_eq!(
            h.port.phase_corr[1],
            (u16::from(phases[3]) << 8) | u16::from(phases[2])
        );
        assert_eq!(h.port.phase_corr[124], u16::from(phases[248]));
    }
}
