pub(crate) use autd3_cpu_wire::payload::{PhaseDepth, WritePatternPhasePayload};

use crate::fpga::{self, EMISSION_RAM};
use crate::port::Port;
use crate::proto::{EMISSION_SLOT_WORDS, Error};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, data) = WritePatternPhasePayload::parse(payload)?;
    let depth = p.depth;
    let intensity = p.intensity;
    let patterns = (u32::from(p.index.get())..)
        .map(|index| index * EMISSION_SLOT_WORDS)
        .zip(data.chunks_exact(depth.bytes_per_pattern()));
    match depth {
        PhaseDepth::Bits8 => fpga::write_ram_at(
            port,
            &EMISSION_RAM,
            p.bank.as_u8(),
            patterns.flat_map(|(offset, pattern)| {
                (offset..).zip(
                    PhaseDepth::unpack_bits8(pattern)
                        .map(move |phase| u16::from_le_bytes([phase, intensity])),
                )
            }),
        ),
        PhaseDepth::Bits4 => fpga::write_ram_at(
            port,
            &EMISSION_RAM,
            p.bank.as_u8(),
            patterns.flat_map(|(offset, pattern)| {
                (offset..).zip(
                    PhaseDepth::unpack_bits4(pattern)
                        .map(move |phase| u16::from_le_bytes([phase, intensity])),
                )
            }),
        ),
        _ => unreachable!(),
    }
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec;
    use std::vec::Vec;

    use autd3_cpu_wire::payload::PhaseDepth;
    use rstest::rstest;

    use crate::fpga::FPGA_PAGE_WORDS;
    use crate::fpga_params::{
        ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, EMISSION_MAX_INDICES, NUM_TRANSDUCERS,
    };
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

    fn bits4_patterns(count: usize) -> Vec<u8> {
        (0..count)
            .flat_map(|g| {
                let phases: [u8; NUM_TRANSDUCERS] =
                    core::array::from_fn(|t| bits4_phase(g, t) * 0x11);
                let mut packed = [0u8; PhaseDepth::Bits4.bytes_per_pattern()];
                PhaseDepth::Bits4.pack(&phases, &mut packed);
                packed
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
            7,
            PhaseDepth::Bits4 as u8,
            count as u8,
            0x42,
            &bits4_patterns(count),
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
    fn write_pattern_phase_bits4_ignores_the_unused_high_nibble_of_the_last_byte() {
        let mut h = Harness::new();
        let data = [0xF5u8; NUM_TRANSDUCERS.div_ceil(2)];

        h.deliver(&write_pattern_phase(
            0,
            0,
            PhaseDepth::Bits4 as u8,
            1,
            0x42,
            &data,
        ));
        assert_eq!(h.status(), Error::None);

        assert_eq!(h.emission_word(0, NUM_TRANSDUCERS - 1), 0x4255);
        assert_eq!(h.emission_word(0, NUM_TRANSDUCERS), 0);
    }

    #[rstest]
    #[case::bits8(PhaseDepth::Bits8)]
    #[case::bits4(PhaseDepth::Bits4)]
    fn a_frame_of_patterns_in_one_page_selects_the_bank_and_the_page_once(
        #[case] depth: PhaseDepth,
    ) {
        let mut h = Harness::new();
        h.port.ram_switches.clear();
        h.port.barriers = 0;
        let count = depth.max_count();
        let data = vec![0x21u8; count * depth.bytes_per_pattern()];
        h.deliver(&write_pattern_phase(
            1,
            5,
            depth as u8,
            count as u8,
            0x80,
            &data,
        ));
        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.port.ram_switches,
            [(ADDR_PATTERN_MEM_WR_BANK, 1), (ADDR_PATTERN_MEM_WR_PAGE, 0)]
        );
        assert_eq!(h.port.barriers, 4);
    }

    #[rstest]
    #[case::bits8(PhaseDepth::Bits8)]
    #[case::bits4(PhaseDepth::Bits4)]
    fn patterns_across_a_page_boundary_land_in_their_slots_with_one_page_switch(
        #[case] depth: PhaseDepth,
    ) {
        let mut h = Harness::new();
        let slot = EMISSION_SLOT_WORDS as usize;
        let per_page = FPGA_PAGE_WORDS as usize / slot;
        let count = depth.max_count();
        let first = per_page - 2;
        let data: Vec<u8> = match depth {
            PhaseDepth::Bits8 => (0..count)
                .flat_map(|g| (0..NUM_TRANSDUCERS).map(move |t| bits8_phase(g, t)))
                .collect(),
            PhaseDepth::Bits4 => bits4_patterns(count),
            _ => unreachable!(),
        };
        let expected = |g: usize, t: usize| match depth {
            PhaseDepth::Bits8 => 0x4200 | u16::from(bits8_phase(g, t)),
            PhaseDepth::Bits4 => 0x4200 | u16::from(bits4_phase(g, t) * 0x11),
            _ => unreachable!(),
        };
        h.port.ram_switches.clear();
        h.port.barriers = 0;
        h.deliver(&write_pattern_phase(
            1,
            first as u16,
            depth as u8,
            count as u8,
            0x42,
            &data,
        ));
        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.port.ram_switches,
            [
                (ADDR_PATTERN_MEM_WR_BANK, 1),
                (ADDR_PATTERN_MEM_WR_PAGE, 0),
                (ADDR_PATTERN_MEM_WR_PAGE, 1)
            ]
        );
        assert_eq!(h.port.barriers, 6);

        let mut reference = Harness::new();
        reference.port.ram_switches.clear();
        for g in 0..count {
            let pattern = &data[g * depth.bytes_per_pattern()..][..depth.bytes_per_pattern()];
            reference.deliver(&write_pattern_phase(
                1,
                (first + g) as u16,
                depth as u8,
                1,
                0x42,
                pattern,
            ));
            assert_eq!(reference.status(), Error::None);
        }
        assert_eq!(reference.port.ram_switches.len(), 2 * count);
        assert_eq!(h.port.em_ram, reference.port.em_ram);
        for g in 0..count {
            for t in 0..NUM_TRANSDUCERS {
                assert_eq!(
                    h.emission_word(1, (first + g) * slot + t),
                    expected(g, t),
                    "pattern {g} transducer {t}"
                );
            }
            for t in NUM_TRANSDUCERS..slot {
                assert_eq!(h.emission_word(1, (first + g) * slot + t), 0);
            }
        }
    }

    #[test]
    fn write_pattern_phase_single_pattern_leaves_the_next_slot_untouched() {
        let mut h = Harness::new();

        h.deliver(&write_pattern_phase(
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
            invalid_bank(),
            0,
            bits8,
            1,
            0xFF,
            &one,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(0, 0, 0, 1, 0xFF, &one));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(0, 0, 3, 1, 0xFF, &one));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(0, 0, bits8, 0, 0xFF, &[]));
        assert_eq!(h.status(), Error::InvalidPayload);

        let bits8_full = vec![0x12u8; PhaseDepth::Bits8.max_count() * NUM_TRANSDUCERS];
        h.deliver(&write_pattern_phase(
            0,
            0,
            bits8,
            PhaseDepth::Bits8.max_count() as u8 + 1,
            0xFF,
            &bits8_full,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        let bits4_full = bits4_patterns(PhaseDepth::Bits4.max_count());
        h.deliver(&write_pattern_phase(
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
            0,
            u16::try_from(EMISSION_MAX_INDICES - 1).unwrap(),
            bits8,
            2,
            0xFF,
            &two,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(0, u16::MAX, bits8, 1, 0xFF, &one));
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
            bits8,
            1,
            0xFF,
            &[0x12; NUM_TRANSDUCERS + 1],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(
            0,
            0,
            bits8,
            2,
            0xFF,
            &[0x12; 2 * NUM_TRANSDUCERS - 1],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_phase(
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
