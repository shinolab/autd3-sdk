use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::frame::PAYLOAD_BYTES;
use crate::layout::OUTPUT_MASK_WORDS;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct OutputMaskPayload {
    pub words: [U16; OUTPUT_MASK_WORDS],
}

impl OutputMaskPayload {
    #[must_use]
    pub fn new(mask: &[bool]) -> Self {
        let mut words = [U16::ZERO; OUTPUT_MASK_WORDS];
        mask.chunks(16)
            .zip(words.iter_mut())
            .for_each(|(chunk, word)| {
                word.set(
                    chunk
                        .iter()
                        .enumerate()
                        .fold(0, |bits, (k, &on)| bits | (u16::from(on) << k)),
                );
            });
        Self { words }
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

const _: () = assert!(core::mem::size_of::<OutputMaskPayload>() <= PAYLOAD_BYTES);

#[cfg(test)]
mod tests {
    use zerocopy::IntoBytes;

    use super::*;
    use crate::fpga_params::NUM_TRANSDUCERS;

    #[test]
    fn transducer_k_is_bit_k_mod_16_of_word_k_div_16() {
        let mut mask = [false; NUM_TRANSDUCERS];
        mask[0] = true;
        mask[15] = true;
        mask[17] = true;
        mask[NUM_TRANSDUCERS - 1] = true;
        let built = OutputMaskPayload::new(&mask);
        assert_eq!(built.words[0].get(), 0x8001);
        assert_eq!(built.words[1].get(), 0x0002);
        assert_eq!(
            built.words[(NUM_TRANSDUCERS - 1) / 16].get(),
            1 << ((NUM_TRANSDUCERS - 1) % 16)
        );
        assert_eq!(
            built
                .words
                .iter()
                .map(|w| w.get().count_ones())
                .sum::<u32>(),
            4
        );
        assert!(OutputMaskPayload::parse(built.as_bytes()).is_ok());
    }

    #[test]
    fn all_enabled_sets_exactly_one_bit_per_transducer() {
        let built = OutputMaskPayload::new(&[true; NUM_TRANSDUCERS]);
        assert_eq!(
            built
                .words
                .iter()
                .map(|w| w.get().count_ones())
                .sum::<u32>(),
            NUM_TRANSDUCERS as u32
        );
    }

    #[test]
    fn transducers_missing_from_a_short_mask_stay_disabled() {
        let built = OutputMaskPayload::new(&[true; 3]);
        assert_eq!(built.words[0].get(), 0b111);
        assert!(built.words[1..].iter().all(|w| w.get() == 0));
    }
}
