pub use autd3_cpu_wire::payload::OutputMaskPayload;

use crate::fpga;
use crate::fpga_params::BramSelect;
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = OutputMaskPayload::parse(payload)?;
    for (j, word) in p.words.iter().enumerate() {
        fpga::write(port, BramSelect::OutputMask, j as u16, word.get());
    }
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec::Vec;

    use crate::fpga_params::NUM_TRANSDUCERS;
    use crate::proto::Error;
    use crate::proto::OUTPUT_MASK_WORDS;
    use crate::test_utils::builders::output_mask;
    use crate::test_utils::mock::Harness;

    #[test]
    fn output_mask_writes_words_verbatim() {
        let mut h = Harness::new();
        let mask: Vec<bool> = (0..NUM_TRANSDUCERS).map(|i| i % 3 == 0).collect();
        h.deliver(&output_mask(0, &mask));
        assert_eq!(h.status(), Error::None);
        for i in 0..OUTPUT_MASK_WORDS {
            let expected = mask[i * 16..]
                .iter()
                .take(16)
                .enumerate()
                .filter(|&(_, &on)| on)
                .fold(0u16, |acc, (j, _)| acc | (1 << j));
            assert_eq!(h.port.output_mask[i], expected);
        }
    }
}
