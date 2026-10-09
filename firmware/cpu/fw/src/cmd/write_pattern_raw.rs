pub use autd3_cpu_wire::payload::WritePatternRawPayload;

use crate::fpga::{self, EMISSION_RAM};
use crate::fpga_params::NUM_TRANSDUCERS;
use crate::port::Port;
use crate::proto::{EMISSION_SLOT_WORDS, Error};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, slots) = WritePatternRawPayload::parse(payload)?;
    let index = u32::from(p.index.get());
    for (k, data) in slots.iter().enumerate() {
        let (phases, intensities) = data.split_at(NUM_TRANSDUCERS);
        fpga::write_ram_interleaved(
            port,
            &EMISSION_RAM,
            p.bank.as_u8(),
            (index + k as u32) * EMISSION_SLOT_WORDS,
            phases.try_into().map_err(|_| Error::InvalidPayload)?,
            intensities.try_into().map_err(|_| Error::InvalidPayload)?,
        );
    }
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec::Vec;

    use crate::fpga::FPGA_PAGE_WORDS;
    use crate::fpga_params::{ADDR_PATTERN_MEM_WR_PAGE, EMISSION_MAX_INDICES, NUM_TRANSDUCERS};
    use crate::proto::{EMISSION_SLOT_WORDS, Error};
    use crate::test_utils::builders::{
        assert_fpga_unchanged, fpga_snapshot, invalid_bank, write_pattern_raw,
        write_pattern_raw_multi,
    };
    use crate::test_utils::mock::Harness;

    fn raw_pattern() -> (Vec<u8>, Vec<u8>) {
        let phases = (0..NUM_TRANSDUCERS).map(|i| i as u8).collect();
        let intensities = (0..NUM_TRANSDUCERS).map(|i| 0xFF - i as u8).collect();
        (phases, intensities)
    }

    #[test]
    fn write_pattern_raw_interleaves_phase_and_intensity_into_slot() {
        let mut h = Harness::new();
        let (phases, intensities) = raw_pattern();

        h.deliver(&write_pattern_raw(0, 1, 3, &phases, &intensities));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.expected_seq(), 1);

        let slot = 3 * EMISSION_SLOT_WORDS as usize;
        for i in 0..NUM_TRANSDUCERS {
            assert_eq!(
                h.emission_word(1, slot + i),
                (u16::from(intensities[i]) << 8) | u16::from(phases[i])
            );
            assert_eq!(h.emission_word(0, slot + i), 0);
        }
        assert_eq!(h.emission_word(1, slot + NUM_TRANSDUCERS), 0);
    }

    #[test]
    fn write_pattern_raw_carries_two_consecutive_indices() {
        let mut h = Harness::new();
        let (phases, intensities) = raw_pattern();
        let reversed: Vec<u8> = phases.iter().rev().copied().collect();

        h.deliver(&write_pattern_raw_multi(
            0,
            0,
            7,
            &[(&phases, &intensities), (&reversed, &phases)],
        ));
        assert_eq!(h.status(), Error::None);

        let first = 7 * EMISSION_SLOT_WORDS as usize;
        let second = 8 * EMISSION_SLOT_WORDS as usize;
        for i in 0..NUM_TRANSDUCERS {
            assert_eq!(
                h.emission_word(0, first + i),
                (u16::from(intensities[i]) << 8) | u16::from(phases[i])
            );
            assert_eq!(
                h.emission_word(0, second + i),
                (u16::from(phases[i]) << 8) | u16::from(reversed[i])
            );
        }
    }

    #[test]
    fn write_pattern_raw_rejects_a_count_out_of_range() {
        let mut h = Harness::new();
        let (phases, intensities) = raw_pattern();
        let before = fpga_snapshot(&h);

        h.deliver(&write_pattern_raw_multi(0, 0, 0, &[]));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_raw_multi(
            1,
            0,
            u16::try_from(EMISSION_MAX_INDICES - 1).unwrap(),
            &[(&phases, &intensities), (&phases, &intensities)],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        let mut frame = write_pattern_raw(2, 0, 0, &phases, &intensities);
        frame.set_payload_byte(1, 3);
        h.deliver(&frame);
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_fpga_unchanged(&before, &h);
    }

    #[test]
    fn write_pattern_raw_selects_page_of_high_index() {
        let mut h = Harness::new();
        let (phases, intensities) = raw_pattern();
        let index = FPGA_PAGE_WORDS / EMISSION_SLOT_WORDS;

        h.deliver(&write_pattern_raw(
            0,
            0,
            u16::try_from(index).unwrap(),
            &phases,
            &intensities,
        ));
        assert_eq!(h.status(), Error::None);

        let slot = (index * EMISSION_SLOT_WORDS) as usize;
        assert_eq!(h.emission_word(0, slot), 0xFF00);
        assert_eq!(h.ctl(ADDR_PATTERN_MEM_WR_PAGE), 1);
    }

    #[test]
    fn write_pattern_raw_rejects_invalid_bank_and_index() {
        let mut h = Harness::new();
        let (phases, intensities) = raw_pattern();
        let before = fpga_snapshot(&h);

        h.deliver(&write_pattern_raw(
            0,
            invalid_bank(),
            0,
            &phases,
            &intensities,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&write_pattern_raw(
            1,
            0,
            u16::try_from(EMISSION_MAX_INDICES).unwrap(),
            &phases,
            &intensities,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_fpga_unchanged(&before, &h);
    }

    #[test]
    fn write_pattern_raw_rejects_a_length_mismatching_count() {
        let mut h = Harness::new();
        let (phases, intensities) = raw_pattern();
        let before = fpga_snapshot(&h);

        let mut long = write_pattern_raw(0, 0, 0, &phases, &intensities);
        long.set_payload_byte(4 + 2 * NUM_TRANSDUCERS, 0);
        h.deliver(&long);
        assert_eq!(h.status(), Error::InvalidPayload);

        let mut short = write_pattern_raw(1, 0, 0, &phases, &intensities);
        short.set_len(4 + 2 * NUM_TRANSDUCERS - 1);
        h.deliver(&short);
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_fpga_unchanged(&before, &h);
    }
}
