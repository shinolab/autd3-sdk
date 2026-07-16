use std::vec;
use std::vec::Vec;

use crate::fpga::FPGA_PAGE_WORDS;
use crate::params::{ADDR_MOD_MEM_WR_PAGE, ADDR_PATTERN_MEM_WR_PAGE, NUM_BANKS, NUM_TRANSDUCERS};
use zerocopy::little_endian::{U16, U32};

use crate::cmd::write_mod::{MOD_WRITE_MAX_DATA_LEN, WriteModPayload};
use crate::cmd::write_pattern::{EM_WRITE_MAX_DATA_LEN, WritePatternPayload};
use crate::cmd::write_pattern_compressed::PatternFormat;
use crate::proto::{Cmd, EMISSION_RAM_WORDS, EMISSION_SLOT_WORDS, Error, MOD_BUFFER_SAMPLES};
use crate::tests::builders::{write_mod_buffer, write_pattern_buffer, write_pattern_compressed};
use crate::tests::mock::{Frame, Harness};

fn bad_bank() -> u8 {
    u8::try_from(NUM_BANKS).unwrap()
}

#[test]
fn write_pattern_buffer_writes_words_at_offset_per_bank() {
    let mut h = Harness::new();

    h.deliver(&write_pattern_buffer(0, 0, 0, &[0x1234, 0x5678]));
    assert_eq!(h.data(), 0);
    h.deliver(&write_pattern_buffer(1, 1, 300, &[0xAABB]));
    assert_eq!(h.data(), 0);
    assert_eq!(h.expected_seq(), 2);

    assert_eq!(h.emission_word(0, 0), 0x1234);
    assert_eq!(h.emission_word(0, 1), 0x5678);
    assert_eq!(h.emission_word(1, 300), 0xAABB);
    assert_eq!(h.emission_word(0, 300), 0);
}

#[test]
fn write_pattern_buffer_crosses_page_boundary() {
    let mut h = Harness::new();

    let page = FPGA_PAGE_WORDS as usize;
    h.deliver(&write_pattern_buffer(
        0,
        0,
        FPGA_PAGE_WORDS - 2,
        &[0x0001, 0x0002, 0x0003, 0x0004],
    ));
    assert_eq!(h.data(), 0);

    assert_eq!(h.emission_word(0, page - 2), 0x0001);
    assert_eq!(h.emission_word(0, page - 1), 0x0002);
    assert_eq!(h.emission_word(0, page), 0x0003);
    assert_eq!(h.emission_word(0, page + 1), 0x0004);
    assert_eq!(h.ctl(ADDR_PATTERN_MEM_WR_PAGE), 1);
}

#[test]
fn write_pattern_buffer_raw_slot_layout() {
    let mut h = Harness::new();

    let pattern: Vec<u16> = (0..NUM_TRANSDUCERS)
        .map(|i| {
            let i = i as u16;
            (i << 8) | (0xFF - (i & 0xFF))
        })
        .collect();
    let slot = 3 * EMISSION_SLOT_WORDS;
    h.deliver(&write_pattern_buffer(0, 0, slot, &pattern));
    assert_eq!(h.data(), 0);

    for (i, w) in pattern.iter().enumerate() {
        assert_eq!(h.emission_word(0, slot as usize + i), *w);
    }
}

#[test]
fn write_pattern_buffer_empty_data_is_no_op_success() {
    let mut h = Harness::new();
    h.deliver(&write_pattern_buffer(0, 0, 0, &[]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.data(), 0);
}

#[test]
fn write_pattern_buffer_rejects_invalid_payloads() {
    let mut h = Harness::new();

    h.deliver(&write_pattern_buffer(0, bad_bank(), 0, &[0x0001]));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    let odd = WritePatternPayload {
        bank: 0,
        reserved: 0,
        offset: U32::new(0),
        data_len: U16::new(3),
    };
    h.deliver(&Frame::from_payload(1, Cmd::WritePatternBuffer, &odd));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    let too_long = WritePatternPayload {
        bank: 0,
        reserved: 0,
        offset: U32::new(0),
        data_len: U16::new(u16::try_from(EM_WRITE_MAX_DATA_LEN + 2).unwrap()),
    };
    h.deliver(&Frame::from_payload(2, Cmd::WritePatternBuffer, &too_long));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_buffer(
        3,
        0,
        EMISSION_RAM_WORDS - 1,
        &[0x0001, 0x0002],
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    assert_eq!(h.emission_word(0, EMISSION_RAM_WORDS as usize - 1), 0);
}

#[test]
fn write_pattern_compressed_phase_full_decompresses_two_indices() {
    let mut h = Harness::new();

    let words: Vec<u16> = (0..NUM_TRANSDUCERS)
        .map(|t| {
            let t = t as u16;
            let p0 = t & 0xFF;
            let p1 = 0xFF - (t & 0xFF);
            p0 | (p1 << 8)
        })
        .collect();
    let slot = 5 * EMISSION_SLOT_WORDS;
    h.deliver(&write_pattern_compressed(
        0,
        1,
        slot,
        PatternFormat::PhaseFull as u8,
        2,
        &words,
    ));
    assert_eq!(h.data(), 0);

    for t in 0..NUM_TRANSDUCERS {
        let p0 = (t & 0xFF) as u16;
        let p1 = 0xFF - (t & 0xFF) as u16;
        assert_eq!(h.emission_word(1, slot as usize + t), 0xFF00 | p0);
        assert_eq!(
            h.emission_word(1, slot as usize + EMISSION_SLOT_WORDS as usize + t),
            0xFF00 | p1
        );
    }
}

#[test]
fn write_pattern_compressed_phase_full_partial_count_writes_single_slot() {
    let mut h = Harness::new();

    let words = vec![0x00AB_u16; NUM_TRANSDUCERS];
    let slot = 2 * EMISSION_SLOT_WORDS;
    h.deliver(&write_pattern_compressed(
        0,
        0,
        slot,
        PatternFormat::PhaseFull as u8,
        1,
        &words,
    ));
    assert_eq!(h.data(), 0);

    assert_eq!(h.emission_word(0, slot as usize), 0xFF00 | 0xAB);
    assert_eq!(
        h.emission_word(0, slot as usize + EMISSION_SLOT_WORDS as usize),
        0
    );
}

#[test]
fn write_pattern_compressed_phase_half_decompresses_four_indices() {
    let mut h = Harness::new();

    let words: Vec<u16> = (0..NUM_TRANSDUCERS)
        .map(|t| {
            let t = t as u16;
            let n0 = t & 0x0F;
            let n1 = (t + 1) & 0x0F;
            let n2 = (t + 2) & 0x0F;
            let n3 = (t + 3) & 0x0F;
            n0 | (n1 << 4) | (n2 << 8) | (n3 << 12)
        })
        .collect();
    let slot = 7 * EMISSION_SLOT_WORDS;
    h.deliver(&write_pattern_compressed(
        0,
        0,
        slot,
        PatternFormat::PhaseHalf as u8,
        4,
        &words,
    ));
    assert_eq!(h.data(), 0);

    for t in 0..NUM_TRANSDUCERS {
        for g in 0..4usize {
            let p4 = ((t + g) & 0x0F) as u16;
            let expected = 0xFF00 | (p4 << 4) | p4;
            assert_eq!(
                h.emission_word(0, slot as usize + g * EMISSION_SLOT_WORDS as usize + t),
                expected
            );
        }
    }
}

#[test]
fn write_pattern_compressed_rejects_invalid_payloads() {
    let mut h = Harness::new();

    let full = vec![0x1234_u16; NUM_TRANSDUCERS];

    h.deliver(&write_pattern_compressed(0, 0, 0, 0, 1, &full));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_compressed(1, 0, 0, 3, 1, &full));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_compressed(
        2,
        0,
        0,
        PatternFormat::PhaseFull as u8,
        0,
        &full,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_compressed(
        3,
        0,
        0,
        PatternFormat::PhaseFull as u8,
        3,
        &full,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_compressed(
        4,
        0,
        0,
        PatternFormat::PhaseHalf as u8,
        5,
        &full,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&write_pattern_compressed(
        5,
        0,
        EMISSION_RAM_WORDS - EMISSION_SLOT_WORDS,
        PatternFormat::PhaseFull as u8,
        2,
        &full,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
}

#[test]
fn write_mod_buffer_packs_samples_into_words_per_bank() {
    let mut h = Harness::new();

    h.deliver(&write_mod_buffer(0, 0, 0, &[0x10, 0x20, 0x30, 0x40]));
    assert_eq!(h.data(), 0);
    h.deliver(&write_mod_buffer(1, 1, 100, &[0xAA, 0xBB]));
    assert_eq!(h.data(), 0);

    assert_eq!(h.mod_word(0, 0), 0x2010);
    assert_eq!(h.mod_word(0, 1), 0x4030);
    assert_eq!(h.mod_word(1, 50), 0xBBAA);
    assert_eq!(h.mod_word(0, 50), 0);
}

#[test]
fn write_mod_buffer_odd_length_pads_high_byte() {
    let mut h = Harness::new();
    h.deliver(&write_mod_buffer(0, 0, 0, &[0xAA]));
    assert_eq!(h.data(), 0);
    assert_eq!(h.mod_word(0, 0), 0x00AA);
}

#[test]
fn write_mod_buffer_crosses_page_boundary() {
    let mut h = Harness::new();

    let offset = 2 * FPGA_PAGE_WORDS - 2;
    h.deliver(&write_mod_buffer(0, 0, offset, &[0x01, 0x02, 0x03, 0x04]));
    assert_eq!(h.data(), 0);

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
    while written < MOD_BUFFER_SAMPLES {
        let len = u32::try_from(MOD_WRITE_MAX_DATA_LEN)
            .unwrap()
            .min(MOD_BUFFER_SAMPLES - written);
        let chunk = vec![(written >> 8) as u8; len as usize];
        h.deliver(&write_mod_buffer(seq, 0, written, &chunk));
        assert_eq!(h.data(), 0);
        seq = seq.wrapping_add(1);
        written += len;
    }
    let expected = ((MOD_BUFFER_SAMPLES - 1) >> 8) as u16;
    let expected = expected | (expected << 8);
    assert_eq!(h.mod_word(0, MOD_BUFFER_SAMPLES as usize / 2 - 1), expected);
}

#[test]
fn write_mod_buffer_empty_data_is_no_op_success() {
    let mut h = Harness::new();
    h.deliver(&write_mod_buffer(0, 0, 0, &[]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.data(), 0);
}

#[test]
fn write_mod_buffer_rejects_invalid_payloads() {
    let mut h = Harness::new();

    h.deliver(&write_mod_buffer(0, bad_bank(), 0, &[0x01]));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&Frame::new(1, Cmd::ReadErrorDetail));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&write_mod_buffer(2, 0, 1, &[0x01, 0x02]));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    let too_long = WriteModPayload {
        bank: 0,
        reserved: 0,
        offset: U32::new(0),
        data_len: U16::new(u16::try_from(MOD_WRITE_MAX_DATA_LEN + 1).unwrap()),
    };
    h.deliver(&Frame::from_payload(
        3,
        Cmd::WriteModulationBuffer,
        &too_long,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&write_mod_buffer(
        4,
        0,
        MOD_BUFFER_SAMPLES - 2,
        &[0x01, 0x02, 0x03],
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    assert_eq!(h.mod_word(0, MOD_BUFFER_SAMPLES as usize / 2 - 1), 0);
}
