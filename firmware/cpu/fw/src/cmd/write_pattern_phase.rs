pub use autd3_cpu_wire::payload::WritePatternPhasePayload;

use crate::fpga::{self, EMISSION_RAM};
use crate::fpga_params::NUM_TRANSDUCERS;
use crate::port::Port;
use crate::proto::{EMISSION_SLOT_WORDS, Error};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, data) = WritePatternPhasePayload::parse(payload)?;
    let index = u32::from(p.index.get());
    let depth = p.depth;
    let intensity = p.intensity;
    for (g, pattern) in data.chunks_exact(depth.bytes_per_pattern()).enumerate() {
        let offset = (index + g as u32) * EMISSION_SLOT_WORDS;
        match depth {
            autd3_cpu_wire::payload::PhaseDepth::Bits8 => fpga::write_ram_words(
                port,
                &EMISSION_RAM,
                p.bank.as_u8(),
                offset,
                pattern
                    .iter()
                    .map(|phase| u16::from_le_bytes([*phase, intensity])),
            ),
            _ => fpga::write_ram_words(
                port,
                &EMISSION_RAM,
                p.bank.as_u8(),
                offset,
                pattern
                    .iter()
                    .flat_map(|byte| [byte & 0x0F, byte >> 4])
                    .take(NUM_TRANSDUCERS)
                    .map(|nibble| u16::from_le_bytes([nibble * 0x11, intensity])),
            ),
        }
    }
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec;
    use std::vec::Vec;

    use autd3_cpu_wire::payload::PhaseDepth;

    use crate::fpga_params::{EMISSION_MAX_INDICES, NUM_TRANSDUCERS};
    use crate::proto::{EMISSION_SLOT_WORDS, Error};
    use crate::test_utils::builders::{
        assert_fpga_unchanged, fpga_snapshot, invalid_bank, write_pattern_phase,
    };
    use crate::test_utils::mock::Harness;

    fn bits8_phase(g: usize, t: usize) -> u8 {
        (t * 3 + g * 17) as u8
    }

    fn bits4_phase(g: usize, t: usize) -> u8 {
        ((t + g) & 0x0F) as u8
    }

    fn pack_bits4(count: usize) -> Vec<u8> {
        (0..count)
            .flat_map(|g| {
                (0..NUM_TRANSDUCERS.div_ceil(2)).map(move |i| {
                    let lo = bits4_phase(g, 2 * i);
                    let hi = if 2 * i + 1 < NUM_TRANSDUCERS {
                        bits4_phase(g, 2 * i + 1)
                    } else {
                        0
                    };
                    (hi << 4) | lo
                })
            })
            .collect()
    }

    #[test]
    fn write_pattern_phase_bits8_writes_consecutive_indices() {
        let mut h = Harness::new();

        let count = PhaseDepth::Bits8.max_count();
        let data: Vec<u8> = (0..count)
            .flat_map(|g| (0..NUM_TRANSDUCERS).map(move |t| bits8_phase(g, t)))
            .collect();
        h.deliver(&write_pattern_phase(
            0,
            1,
            5,
            PhaseDepth::Bits8 as u8,
            count as u8,
            0x80,
            &data,
        ));
        assert_eq!(h.status(), Error::None);

        let slot = EMISSION_SLOT_WORDS as usize;
        for g in 0..count {
            for t in 0..NUM_TRANSDUCERS {
                assert_eq!(
                    h.emission_word(1, (5 + g) * slot + t),
                    0x8000 | u16::from(bits8_phase(g, t)),
                    "pattern {g} transducer {t}"
                );
            }
        }
        assert_eq!(h.emission_word(0, 5 * slot), 0);
    }

    #[test]
    fn write_pattern_phase_bits4_unpacks_nibbles_to_full_range() {
        let mut h = Harness::new();

        let count = PhaseDepth::Bits4.max_count();
        h.deliver(&write_pattern_phase(
            0,
            0,
            7,
            PhaseDepth::Bits4 as u8,
            count as u8,
            0x42,
            &pack_bits4(count),
        ));
        assert_eq!(h.status(), Error::None);

        let slot = EMISSION_SLOT_WORDS as usize;
        for g in 0..count {
            for t in 0..NUM_TRANSDUCERS {
                assert_eq!(
                    h.emission_word(0, (7 + g) * slot + t),
                    0x4200 | u16::from(bits4_phase(g, t) * 0x11),
                    "pattern {g} transducer {t}"
                );
            }
        }
    }

    #[test]
    fn write_pattern_phase_single_pattern_leaves_the_next_slot_untouched() {
        let mut h = Harness::new();

        h.deliver(&write_pattern_phase(
            0,
            0,
            2,
            PhaseDepth::Bits8 as u8,
            1,
            0xFF,
            &[0xAB; NUM_TRANSDUCERS],
        ));
        assert_eq!(h.status(), Error::None);

        let slot = EMISSION_SLOT_WORDS as usize;
        assert_eq!(h.emission_word(0, 2 * slot), 0xFFAB);
        assert_eq!(h.emission_word(0, 2 * slot + NUM_TRANSDUCERS - 1), 0xFFAB);
        assert_eq!(h.emission_word(0, 3 * slot), 0);
    }

    #[test]
    fn write_pattern_phase_rejects_invalid_payloads() {
        let mut h = Harness::new();
        let one = [0x12u8; NUM_TRANSDUCERS];
        let bits8 = PhaseDepth::Bits8 as u8;
        let before = fpga_snapshot(&h);

        h.deliver(&write_pattern_phase(
            0,
            invalid_bank(),
            0,
            bits8,
            1,
            0xFF,
            &one,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(1, 0, 0, 0, 1, 0xFF, &one));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(2, 0, 0, 3, 1, 0xFF, &one));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(3, 0, 0, bits8, 0, 0xFF, &[]));
        assert_eq!(h.status(), Error::InvalidPayload);

        let bits8_full = vec![0x12u8; PhaseDepth::Bits8.max_count() * NUM_TRANSDUCERS];
        h.deliver(&write_pattern_phase(
            4,
            0,
            0,
            bits8,
            PhaseDepth::Bits8.max_count() as u8 + 1,
            0xFF,
            &bits8_full,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        let bits4_full = pack_bits4(PhaseDepth::Bits4.max_count());
        h.deliver(&write_pattern_phase(
            5,
            0,
            0,
            PhaseDepth::Bits4 as u8,
            PhaseDepth::Bits4.max_count() as u8 + 1,
            0xFF,
            &bits4_full,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        let two = [0x12u8; 2 * NUM_TRANSDUCERS];
        h.deliver(&write_pattern_phase(
            6,
            0,
            u16::try_from(EMISSION_MAX_INDICES - 1).unwrap(),
            bits8,
            2,
            0xFF,
            &two,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(7, 0, u16::MAX, bits8, 1, 0xFF, &one));
        assert_eq!(h.status(), Error::InvalidPayload);

        assert_fpga_unchanged(&before, &h);
    }

    #[test]
    fn write_pattern_phase_rejects_a_length_mismatching_count() {
        let mut h = Harness::new();
        let bits8 = PhaseDepth::Bits8 as u8;
        let before = fpga_snapshot(&h);

        h.deliver(&write_pattern_phase(
            0,
            0,
            0,
            bits8,
            1,
            0xFF,
            &[0x12; NUM_TRANSDUCERS + 1],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(
            1,
            0,
            0,
            bits8,
            2,
            0xFF,
            &[0x12; 2 * NUM_TRANSDUCERS - 1],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(
            2,
            0,
            0,
            PhaseDepth::Bits4 as u8,
            1,
            0xFF,
            &[0x12; NUM_TRANSDUCERS],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_fpga_unchanged(&before, &h);
    }
}
