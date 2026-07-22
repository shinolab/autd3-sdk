use std::vec::Vec;

use crate::params::{
    ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, ADDR_MOD_REQ_RD_BANK,
    ADDR_MOD_TRANSITION_MODE, ADDR_MOD_TRANSITION_VALUE_0, ADDR_PATTERN_CYCLE0,
    ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MODE0, ADDR_PATTERN_NUM_FOCI0, ADDR_PATTERN_REP0,
    ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_SOUND_SPEED0, ADDR_PATTERN_TRANSITION_MODE,
    ADDR_PATTERN_TRANSITION_VALUE_0, CTL_FLAG_MOD_SET, CTL_FLAG_PATTERN_SET, EMISSION_TYPE_FOCI,
    EMISSION_TYPE_RAW, NUM_BANKS, NUM_TRANSDUCERS,
};

use crate::fpga::TransitionMode;
use crate::proto::{EMISSION_SLOT_WORDS, Error};
use crate::tests::builders::{
    FusedMod, FusedPattern, change_mod_bank, change_pattern_bank, config_mod_rep,
    config_pattern_rep, write_mod_buffer, write_mod_fused, write_pattern_buffer,
    write_pattern_fused,
};
use crate::tests::mock::Harness;

fn pattern_words() -> Vec<u16> {
    (0..NUM_TRANSDUCERS)
        .map(|i| {
            let i = i as u16;
            (i << 8) | (0xA5 ^ (i & 0xFF))
        })
        .collect()
}

fn pattern_state(h: &Harness, bank: u8, words: usize) -> Vec<u16> {
    let b = u16::from(bank);
    let mut s = Vec::new();
    s.extend([
        h.ctl(ADDR_PATTERN_MODE0 + b),
        h.ctl(ADDR_PATTERN_CYCLE0 + b),
        h.ctl(ADDR_PATTERN_FREQ_DIV0 + b),
        h.ctl(ADDR_PATTERN_SOUND_SPEED0 + b),
        h.ctl(ADDR_PATTERN_NUM_FOCI0 + b),
        h.ctl(ADDR_PATTERN_REP0 + b),
        h.ctl(ADDR_PATTERN_REQ_RD_BANK),
        h.ctl(ADDR_PATTERN_TRANSITION_MODE),
    ]);
    s.extend((0..4).map(|i| h.ctl(ADDR_PATTERN_TRANSITION_VALUE_0 + i)));
    s.extend((0..words).map(|i| h.emission_word(bank, i)));
    s
}

fn mod_state(h: &Harness, bank: u8, samples: usize) -> Vec<u16> {
    let b = u16::from(bank);
    let mut s = std::vec![
        h.ctl(ADDR_MOD_CYCLE0 + b),
        h.ctl(ADDR_MOD_FREQ_DIV0 + b),
        h.ctl(ADDR_MOD_REP0 + b),
        h.ctl(ADDR_MOD_REQ_RD_BANK),
        h.ctl(ADDR_MOD_TRANSITION_MODE),
    ];
    s.extend((0..4).map(|i| h.ctl(ADDR_MOD_TRANSITION_VALUE_0 + i)));
    s.extend((0..samples.div_ceil(2)).map(|i| h.mod_word(bank, i)));
    s
}

#[test]
fn fused_pattern_matches_three_frame_path_bit_for_bit() {
    let words = pattern_words();
    let bank = 1;
    let divider = 20;

    let mut split = Harness::new();
    split.deliver(&write_pattern_buffer(0, bank, 0, &words));
    split.deliver(&config_pattern_rep(
        1,
        bank,
        EMISSION_TYPE_RAW,
        divider,
        1,
        0,
        0,
        crate::fpga::REP_INFINITE,
    ));
    split.deliver(&change_pattern_bank(2, bank, TransitionMode::Immediate, 0));
    assert_eq!(split.data(), 0);

    let mut fused = Harness::new();
    fused.deliver(&write_pattern_fused(
        0,
        &FusedPattern::raw(bank, divider, 1),
        &words,
    ));
    assert_eq!(fused.data(), 0, "fused frame must succeed");

    assert_eq!(
        pattern_state(&split, bank, EMISSION_SLOT_WORDS as usize),
        pattern_state(&fused, bank, EMISSION_SLOT_WORDS as usize),
        "fused pattern must leave identical FPGA state"
    );
    let base = Harness::new().latch_count(CTL_FLAG_PATTERN_SET);
    assert_eq!(
        split.latch_count(CTL_FLAG_PATTERN_SET) - base,
        2,
        "split path latches once for config and once for the bank change"
    );
    assert_eq!(
        fused.latch_count(CTL_FLAG_PATTERN_SET) - base,
        1,
        "fused path latches exactly once: one UPDATE applies config and bank switch together"
    );
    assert_eq!(fused.expected_seq(), 1, "fused update costs a single frame");
    assert_eq!(split.expected_seq(), 3, "split update costs three frames");
}

#[test]
fn fused_foci_matches_three_frame_path_bit_for_bit() {
    let foci: Vec<u16> = (0..40u16).map(|i| i.wrapping_mul(0x1357)).collect();
    let bank = 0;
    let divider = 10;
    let size = 10;
    let num_foci = 1;
    let sound_speed = 21760;

    let mut split = Harness::new();
    split.deliver(&write_pattern_buffer(0, bank, 0, &foci));
    split.deliver(&config_pattern_rep(
        1,
        bank,
        EMISSION_TYPE_FOCI,
        divider,
        size,
        num_foci,
        sound_speed,
        crate::fpga::REP_INFINITE,
    ));
    split.deliver(&change_pattern_bank(2, bank, TransitionMode::Immediate, 0));
    assert_eq!(split.data(), 0);

    let mut fused = Harness::new();
    let mut f = FusedPattern::raw(bank, divider, size);
    f.emission_type = EMISSION_TYPE_FOCI;
    f.num_foci = num_foci;
    f.sound_speed = sound_speed;
    fused.deliver(&write_pattern_fused(0, &f, &foci));
    assert_eq!(fused.data(), 0);

    assert_eq!(
        pattern_state(&split, bank, foci.len()),
        pattern_state(&fused, bank, foci.len())
    );
    assert_eq!(fused.expected_seq(), 1);
}

#[test]
fn fused_modulation_matches_three_frame_path_bit_for_bit() {
    let data: Vec<u8> = (0..64u16).map(|i| (i ^ 0x5A) as u8).collect();
    let bank = 1;
    let divider = 10;

    let mut split = Harness::new();
    split.deliver(&write_mod_buffer(0, bank, 0, &data));
    split.deliver(&config_mod_rep(
        1,
        bank,
        divider,
        data.len() as u32,
        crate::fpga::REP_INFINITE,
    ));
    split.deliver(&change_mod_bank(2, bank, TransitionMode::Immediate, 0));
    assert_eq!(split.data(), 0);

    let mut fused = Harness::new();
    fused.deliver(&write_mod_fused(
        0,
        &FusedMod::new(bank, divider, data.len() as u32),
        &data,
    ));
    assert_eq!(fused.data(), 0);

    assert_eq!(
        mod_state(&split, bank, data.len()),
        mod_state(&fused, bank, data.len())
    );
    let base = Harness::new().latch_count(CTL_FLAG_MOD_SET);
    assert_eq!(split.latch_count(CTL_FLAG_MOD_SET) - base, 2);
    assert_eq!(
        fused.latch_count(CTL_FLAG_MOD_SET) - base,
        1,
        "fused path latches exactly once"
    );
    assert_eq!(fused.expected_seq(), 1);
}

#[test]
fn fused_pattern_rejects_bad_bank() {
    let mut h = Harness::new();
    let bad = u8::try_from(NUM_BANKS).unwrap();
    h.deliver(&write_pattern_fused(
        0,
        &FusedPattern::raw(bad, 10, 1),
        &[0x1234],
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
}

#[test]
fn fused_pattern_rejects_zero_size_without_writing_config() {
    let mut h = Harness::new();
    h.deliver(&write_pattern_fused(
        0,
        &FusedPattern::raw(0, 10, 0),
        &[0x1234],
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
}

#[test]
fn fused_modulation_rejects_bad_bank() {
    let mut h = Harness::new();
    let bad = u8::try_from(NUM_BANKS).unwrap();
    h.deliver(&write_mod_fused(
        0,
        &FusedMod::new(bad, 10, 4),
        &[1, 2, 3, 4],
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
}

#[test]
fn fused_pattern_rejects_invalid_transition_mode_for_finite_loop() {
    let mut h = Harness::new();
    let mut f = FusedPattern::raw(1, 10, 2);
    f.rep = 4;
    f.transition_mode = TransitionMode::Immediate;
    h.deliver(&write_pattern_fused(0, &f, &pattern_words()));
    assert_eq!(h.data(), Error::InvalidTransitionMode as u8);
}
