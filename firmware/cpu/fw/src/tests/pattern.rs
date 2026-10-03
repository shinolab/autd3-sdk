use std::vec;
use std::vec::Vec;

use crate::fpga::FPGA_PAGE_WORDS;
use crate::params::{
    ADDR_MOD_MEM_WR_PAGE, ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, EMISSION_MAX_INDICES,
    NUM_BANKS, NUM_TRANSDUCERS,
};
use zerocopy::little_endian::U32;

use crate::cmd::write_foci::WriteFociPayload;
use crate::proto::{Cmd, EMISSION_RAM_WORDS, EMISSION_SLOT_WORDS, Error, MOD_BUFFER_SAMPLES};
use crate::tests::builders::{
    assert_fpga_unchanged, fpga_snapshot, write_foci_buffer, write_mod_buffer, write_pattern_phase,
    write_pattern_raw, write_pattern_raw_multi,
};
use crate::tests::mock::{Frame, Harness};
use autd3_cpu_wire::payload::{PhaseDepth, WriteModPayload};
use autd3_cpu_wire::{PAYLOAD_BYTES, PatternBank};

fn bad_bank() -> u8 {
    u8::try_from(NUM_BANKS).unwrap()
}

#[test]
fn write_foci_buffer_writes_words_at_offset_per_bank() {
    let mut h = Harness::new();

    h.deliver(&write_foci_buffer(0, 0, 0, &[0x1234, 0x5678]));
    assert_eq!(h.status(), 0);
    h.deliver(&write_foci_buffer(1, 1, 300, &[0xAABB]));
    assert_eq!(h.status(), 0);
    assert_eq!(h.expected_seq(), 2);

    assert_eq!(h.emission_word(0, 0), 0x1234);
    assert_eq!(h.emission_word(0, 1), 0x5678);
    assert_eq!(h.emission_word(1, 300), 0xAABB);
    assert_eq!(h.emission_word(0, 300), 0);
}

#[test]
fn write_foci_buffer_crosses_page_boundary() {
    let mut h = Harness::new();

    let page = FPGA_PAGE_WORDS as usize;
    h.deliver(&write_foci_buffer(
        0,
        0,
        FPGA_PAGE_WORDS - 2,
        &[0x0001, 0x0002, 0x0003, 0x0004],
    ));
    assert_eq!(h.status(), 0);

    assert_eq!(h.emission_word(0, page - 2), 0x0001);
    assert_eq!(h.emission_word(0, page - 1), 0x0002);
    assert_eq!(h.emission_word(0, page), 0x0003);
    assert_eq!(h.emission_word(0, page + 1), 0x0004);
    assert_eq!(h.ctl(ADDR_PATTERN_MEM_WR_PAGE), 1);
}

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
    assert_eq!(h.status(), 0);
    assert_eq!(h.expected_seq(), 1);

    let slot = 3 * EMISSION_SLOT_WORDS as usize;
    for i in 0..NUM_TRANSDUCERS {
        assert_eq!(
            h.emission_word(1, slot + i),
            u16::from(phases[i]) | (u16::from(intensities[i]) << 8)
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
    assert_eq!(h.status(), 0);

    let first = 7 * EMISSION_SLOT_WORDS as usize;
    let second = 8 * EMISSION_SLOT_WORDS as usize;
    for i in 0..NUM_TRANSDUCERS {
        assert_eq!(
            h.emission_word(0, first + i),
            u16::from(phases[i]) | (u16::from(intensities[i]) << 8)
        );
        assert_eq!(
            h.emission_word(0, second + i),
            u16::from(reversed[i]) | (u16::from(phases[i]) << 8)
        );
    }
}

#[test]
fn write_pattern_raw_rejects_a_count_out_of_range() {
    let mut h = Harness::new();
    let (phases, intensities) = raw_pattern();
    let before = fpga_snapshot(&h);

    h.deliver(&write_pattern_raw_multi(0, 0, 0, &[]));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_raw_multi(
        1,
        0,
        u16::try_from(EMISSION_MAX_INDICES - 1).unwrap(),
        &[(&phases, &intensities), (&phases, &intensities)],
    ));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    let mut frame = write_pattern_raw(2, 0, 0, &phases, &intensities);
    frame.set_payload_byte(1, 3);
    h.deliver(&frame);
    assert_eq!(h.status(), Error::InvalidPayload as u8);
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
    assert_eq!(h.status(), 0);

    let slot = (index * EMISSION_SLOT_WORDS) as usize;
    assert_eq!(h.emission_word(0, slot), 0xFF00);
    assert_eq!(h.ctl(ADDR_PATTERN_MEM_WR_PAGE), 1);
}

#[test]
fn write_pattern_raw_rejects_invalid_bank_and_index() {
    let mut h = Harness::new();
    let (phases, intensities) = raw_pattern();
    let before = fpga_snapshot(&h);

    h.deliver(&write_pattern_raw(0, bad_bank(), 0, &phases, &intensities));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_raw(
        1,
        0,
        u16::try_from(EMISSION_MAX_INDICES).unwrap(),
        &phases,
        &intensities,
    ));
    assert_eq!(h.status(), Error::InvalidPayload as u8);
    assert_fpga_unchanged(&before, &h);
}

#[test]
fn write_foci_buffer_empty_data_is_no_op_success() {
    let mut h = Harness::new();
    h.deliver(&write_foci_buffer(0, 0, 0, &[]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.status(), 0);
}

#[test]
fn write_foci_buffer_empty_data_at_ram_end_does_not_switch_bank_or_page() {
    const UNTOUCHED: u16 = 0xBEEF;
    let mut h = Harness::new();
    h.set_ctl(ADDR_PATTERN_MEM_WR_BANK, UNTOUCHED);
    h.set_ctl(ADDR_PATTERN_MEM_WR_PAGE, UNTOUCHED);

    h.deliver(&write_foci_buffer(0, 1, EMISSION_RAM_WORDS, &[]));

    assert_eq!(h.ack(), 0);
    assert_eq!(h.status(), 0);
    assert_eq!(h.ctl(ADDR_PATTERN_MEM_WR_BANK), UNTOUCHED);
    assert_eq!(h.ctl(ADDR_PATTERN_MEM_WR_PAGE), UNTOUCHED);
}

#[test]
fn write_foci_buffer_rejects_invalid_payloads() {
    let mut h = Harness::new();

    h.deliver(&write_foci_buffer(0, bad_bank(), 0, &[0x0001]));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    let odd = WriteFociPayload {
        bank: PatternBank::B0,
        reserved: 0,
        offset: U32::new(0),
    };
    h.deliver(&Frame::from_parts(
        1,
        Cmd::WriteFociBuffer,
        &odd,
        &[0x01, 0x02, 0x03],
    ));
    assert_eq!(h.status(), Error::InvalidPayload as u8);
    assert_eq!(h.emission_word(0, 0), 0);

    h.deliver(&write_foci_buffer(
        3,
        0,
        EMISSION_RAM_WORDS - 1,
        &[0x0001, 0x0002],
    ));
    assert_eq!(h.status(), Error::InvalidPayload as u8);
    assert_eq!(h.emission_word(0, EMISSION_RAM_WORDS as usize - 1), 0);

    let before = fpga_snapshot(&h);
    h.deliver(&write_foci_buffer(4, 0, u32::MAX, &[0x0001]));
    assert_eq!(h.status(), Error::InvalidPayload as u8);
    assert_fpga_unchanged(&before, &h);
}

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
                lo | (hi << 4)
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
    assert_eq!(h.status(), 0);

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
    assert_eq!(h.status(), 0);

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
    assert_eq!(h.status(), 0);

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

    h.deliver(&write_pattern_phase(0, bad_bank(), 0, bits8, 1, 0xFF, &one));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_phase(1, 0, 0, 0, 1, 0xFF, &one));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_phase(2, 0, 0, 3, 1, 0xFF, &one));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_phase(3, 0, 0, bits8, 0, 0xFF, &[]));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

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
    assert_eq!(h.status(), Error::InvalidPayload as u8);

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
    assert_eq!(h.status(), Error::InvalidPayload as u8);

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
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_phase(7, 0, u16::MAX, bits8, 1, 0xFF, &one));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    assert_fpga_unchanged(&before, &h);
}

#[test]
fn write_mod_buffer_packs_samples_into_words_per_bank() {
    let mut h = Harness::new();

    h.deliver(&write_mod_buffer(0, 0, 0, &[0x10, 0x20, 0x30, 0x40]));
    assert_eq!(h.status(), 0);
    h.deliver(&write_mod_buffer(1, 1, 100, &[0xAA, 0xBB]));
    assert_eq!(h.status(), 0);

    assert_eq!(h.mod_word(0, 0), 0x2010);
    assert_eq!(h.mod_word(0, 1), 0x4030);
    assert_eq!(h.mod_word(1, 50), 0xBBAA);
    assert_eq!(h.mod_word(0, 50), 0);
}

#[test]
fn write_mod_buffer_odd_length_pads_high_byte() {
    let mut h = Harness::new();
    h.deliver(&write_mod_buffer(0, 0, 0, &[0xAA]));
    assert_eq!(h.status(), 0);
    assert_eq!(h.mod_word(0, 0), 0x00AA);
}

#[test]
fn write_mod_buffer_writes_trailing_zero_samples() {
    let mut h = Harness::new();
    h.deliver(&write_mod_buffer(0, 0, 0, &[0x11, 0x22, 0x33, 0x44]));
    h.deliver(&write_mod_buffer(1, 0, 0, &[0x55, 0x00, 0x00, 0x00]));
    assert_eq!(h.status(), 0);
    assert_eq!(h.mod_word(0, 0), 0x0055);
    assert_eq!(h.mod_word(0, 1), 0x0000);
}

#[test]
fn write_pattern_raw_rejects_a_length_mismatching_count() {
    let mut h = Harness::new();
    let (phases, intensities) = raw_pattern();
    let before = fpga_snapshot(&h);

    let mut long = write_pattern_raw(0, 0, 0, &phases, &intensities);
    long.set_payload_byte(4 + 2 * NUM_TRANSDUCERS, 0);
    h.deliver(&long);
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    let mut short = write_pattern_raw(1, 0, 0, &phases, &intensities);
    short.set_len(4 + 2 * NUM_TRANSDUCERS - 1);
    h.deliver(&short);
    assert_eq!(h.status(), Error::InvalidPayload as u8);
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
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_phase(
        1,
        0,
        0,
        bits8,
        2,
        0xFF,
        &[0x12; 2 * NUM_TRANSDUCERS - 1],
    ));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_phase(
        2,
        0,
        0,
        PhaseDepth::Bits4 as u8,
        1,
        0xFF,
        &[0x12; NUM_TRANSDUCERS],
    ));
    assert_eq!(h.status(), Error::InvalidPayload as u8);
    assert_fpga_unchanged(&before, &h);
}

#[test]
fn write_mod_buffer_crosses_page_boundary() {
    let mut h = Harness::new();

    let offset = 2 * FPGA_PAGE_WORDS - 2;
    h.deliver(&write_mod_buffer(0, 0, offset, &[0x01, 0x02, 0x03, 0x04]));
    assert_eq!(h.status(), 0);

    let page = FPGA_PAGE_WORDS as usize;
    assert_eq!(h.mod_word(0, page - 1), 0x0201);
    assert_eq!(h.mod_word(0, page), 0x0403);
    assert_eq!(h.ctl(ADDR_MOD_MEM_WR_PAGE), 1);
}

#[test]
fn write_mod_buffer_accepts_chunked_writes_up_to_capacity() {
    let mut h = Harness::new();

    let mut seq: u8 = 0;
    let mut written: u32 = 0;
    let mut last = 0u8;
    while written < MOD_BUFFER_SAMPLES {
        let len = u32::try_from(PAYLOAD_BYTES - size_of::<WriteModPayload>())
            .unwrap()
            .min(MOD_BUFFER_SAMPLES - written);
        last = (written >> 8) as u8;
        let chunk = vec![last; len as usize];
        h.deliver(&write_mod_buffer(seq, 0, written, &chunk));
        assert_eq!(h.status(), 0);
        seq = seq.wrapping_add(1);
        written += len;
    }
    let expected = u16::from(last) | (u16::from(last) << 8);
    assert_eq!(h.mod_word(0, MOD_BUFFER_SAMPLES as usize / 2 - 1), expected);
}

#[test]
fn write_mod_buffer_empty_data_is_no_op_success() {
    let mut h = Harness::new();
    h.deliver(&write_mod_buffer(0, 0, 0, &[]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.status(), 0);
}

#[test]
fn write_mod_buffer_rejects_invalid_payloads() {
    let mut h = Harness::new();

    h.deliver(&write_mod_buffer(0, bad_bank(), 0, &[0x01]));
    assert_eq!(h.status(), Error::InvalidPayload as u8);
    h.deliver(&Frame::new(1, Cmd::ReadErrorDetail));
    assert_eq!(h.reply_data(), [Error::InvalidPayload as u8]);

    h.deliver(&write_mod_buffer(2, 0, 1, &[0x01, 0x02]));
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    h.deliver(&write_mod_buffer(
        4,
        0,
        MOD_BUFFER_SAMPLES - 2,
        &[0x01, 0x02, 0x03],
    ));
    assert_eq!(h.status(), Error::InvalidPayload as u8);
    assert_eq!(h.mod_word(0, MOD_BUFFER_SAMPLES as usize / 2 - 1), 0);

    let before = fpga_snapshot(&h);
    h.deliver(&write_mod_buffer(5, 0, u32::MAX - 1, &[0x01, 0x02]));
    assert_eq!(h.status(), Error::InvalidPayload as u8);
    assert_fpga_unchanged(&before, &h);
}
