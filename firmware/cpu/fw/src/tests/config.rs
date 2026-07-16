use crate::fpga::{REP_INFINITE, SYS_TIME_TRANSITION_MARGIN_NS, TransitionMode};
use crate::params::{
    ADDR_CTL_FLAG, ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, ADDR_MOD_REQ_RD_BANK,
    ADDR_MOD_TRANSITION_MODE, ADDR_PATTERN_CYCLE0, ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MODE0,
    ADDR_PATTERN_NUM_FOCI0, ADDR_PATTERN_REP0, ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_SOUND_SPEED0,
    ADDR_PATTERN_TRANSITION_MODE, ADDR_PATTERN_TRANSITION_VALUE_0, CTL_FLAG_MOD_SET,
    CTL_FLAG_PATTERN_SET, EMISSION_MAX_INDICES, EMISSION_TYPE_FOCI, EMISSION_TYPE_RAW, NUM_BANKS,
    NUM_FOCI_MAX,
};
use crate::proto::{Error, MAX_FOCI_TOTAL, MOD_BUFFER_SAMPLES};
use crate::tests::builders::{
    change_mod_bank, change_pattern_bank, config_mod, config_mod_rep, config_pattern,
    config_pattern_rep,
};
use crate::tests::mock::Harness;

fn invalid_bank() -> u8 {
    u8::try_from(NUM_BANKS).unwrap()
}

#[test]
fn config_mod_writes_playback_registers_and_latches() {
    let mut h = Harness::new();
    let latches_at_boot = h.latch_count(CTL_FLAG_MOD_SET);

    h.deliver(&config_mod(0, 1, 10, 4000));

    assert_eq!(h.ack(), 0);
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_CYCLE0 + 1), 3999);
    assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0 + 1), 10);
    assert_eq!(h.ctl(ADDR_MOD_REP0 + 1), REP_INFINITE);
    assert_eq!(
        h.ctl(ADDR_MOD_TRANSITION_MODE),
        TransitionMode::SyncIdx as u16
    );
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);
    assert_eq!(h.latch_count(CTL_FLAG_MOD_SET), latches_at_boot + 1);
    assert_eq!(h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_MOD_SET, 0);
}

#[test]
fn config_mod_writes_finite_loop_rep() {
    let mut h = Harness::new();

    h.deliver(&config_mod_rep(0, 0, 10, 4000, 9));

    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_REP0), 9);
}

#[test]
fn config_mod_rejects_invalid_fields_and_leaves_registers_untouched() {
    let mut h = Harness::new();
    h.deliver(&config_mod(0, 1, 2, 100));
    assert_eq!(h.data(), 0);

    h.deliver(&config_mod(1, invalid_bank(), 1, 1));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&config_mod(2, 0, 0, 1));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&config_mod(3, 0, 1, 0));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&config_mod(4, 0, 1, MOD_BUFFER_SAMPLES + 1));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    assert_eq!(h.ctl(ADDR_MOD_CYCLE0 + 1), 99);
    assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0 + 1), 2);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);
}

#[test]
fn config_mod_accepts_full_buffer_size() {
    let mut h = Harness::new();
    h.deliver(&config_mod(0, 0, 1, MOD_BUFFER_SAMPLES));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_CYCLE0), 0xFFFF);
}

#[test]
fn config_pattern_raw_writes_registers_and_latches() {
    let mut h = Harness::new();

    h.deliver(&config_pattern(
        0,
        0,
        EMISSION_TYPE_RAW,
        2,
        EMISSION_MAX_INDICES,
        0,
        0,
    ));

    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_PATTERN_MODE0), u16::from(EMISSION_TYPE_RAW));
    assert_eq!(
        h.ctl(ADDR_PATTERN_CYCLE0),
        u16::try_from(EMISSION_MAX_INDICES - 1).unwrap()
    );
    assert_eq!(h.ctl(ADDR_PATTERN_FREQ_DIV0), 2);
    assert_eq!(h.ctl(ADDR_PATTERN_REP0), REP_INFINITE);
    assert_eq!(
        h.ctl(ADDR_PATTERN_TRANSITION_MODE),
        TransitionMode::SyncIdx as u16
    );
    assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);
    assert_eq!(h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_PATTERN_SET, 0);
}

#[test]
fn config_pattern_foci_writes_registers_and_latches() {
    let mut h = Harness::new();

    h.deliver(&config_pattern(0, 1, EMISSION_TYPE_FOCI, 1, 8192, 8, 340));

    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_PATTERN_MODE0 + 1), u16::from(EMISSION_TYPE_FOCI));
    assert_eq!(h.ctl(ADDR_PATTERN_CYCLE0 + 1), 8191);
    assert_eq!(h.ctl(ADDR_PATTERN_SOUND_SPEED0 + 1), 340);
    assert_eq!(h.ctl(ADDR_PATTERN_NUM_FOCI0 + 1), 8);
    assert_eq!(h.ctl(ADDR_PATTERN_REP0 + 1), REP_INFINITE);
    assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);
    assert_eq!(h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_PATTERN_SET, 0);
}

#[test]
fn config_pattern_writes_finite_loop_rep() {
    let mut h = Harness::new();

    h.deliver(&config_pattern_rep(
        0,
        0,
        EMISSION_TYPE_RAW,
        2,
        EMISSION_MAX_INDICES,
        0,
        0,
        4,
    ));

    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_PATTERN_REP0), 4);
}

#[test]
fn config_pattern_rejects_invalid_raw_fields() {
    let mut h = Harness::new();

    h.deliver(&config_pattern(
        0,
        0,
        EMISSION_TYPE_RAW,
        1,
        EMISSION_MAX_INDICES + 1,
        0,
        0,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&config_pattern(1, 0, 2, 1, 1, 0, 0));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    assert_eq!(h.ctl(ADDR_PATTERN_CYCLE0), 0);
}

#[test]
fn config_pattern_rejects_invalid_foci_fields() {
    let mut h = Harness::new();

    h.deliver(&config_pattern(0, 0, EMISSION_TYPE_FOCI, 1, 1, 0, 340));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&config_pattern(
        1,
        0,
        EMISSION_TYPE_FOCI,
        1,
        1,
        NUM_FOCI_MAX + 1,
        340,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&config_pattern(
        2,
        0,
        EMISSION_TYPE_FOCI,
        1,
        MAX_FOCI_TOTAL / 8 + 1,
        8,
        340,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&config_pattern(3, 0, EMISSION_TYPE_FOCI, 1, 1, 1, 0));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    h.deliver(&config_pattern(
        4,
        0,
        EMISSION_TYPE_FOCI,
        1,
        MAX_FOCI_TOTAL / 8,
        8,
        340,
    ));
    assert_eq!(h.data(), 0);
}

#[test]
fn change_pattern_bank_writes_transition_and_req_bank_and_latches() {
    let mut h = Harness::new();
    let latches_at_boot = h.latch_count(CTL_FLAG_PATTERN_SET);

    h.deliver(&change_pattern_bank(0, 1, TransitionMode::Immediate, 0));

    assert_eq!(h.data(), 0);
    assert_eq!(
        h.ctl(ADDR_PATTERN_TRANSITION_MODE),
        TransitionMode::Immediate as u16
    );
    assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
    assert_eq!(h.latch_count(CTL_FLAG_PATTERN_SET), latches_at_boot + 1);
    assert_eq!(h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_PATTERN_SET, 0);
}

#[test]
fn change_pattern_bank_writes_transition_value() {
    let mut h = Harness::new();

    h.deliver(&config_pattern_rep(
        0,
        0,
        EMISSION_TYPE_RAW,
        2,
        EMISSION_MAX_INDICES,
        0,
        0,
        4,
    ));
    assert_eq!(h.data(), 0);

    h.deliver(&change_pattern_bank(
        1,
        0,
        TransitionMode::SysTime,
        0x0123_4567_89AB_CDEF,
    ));

    assert_eq!(h.data(), 0);
    assert_eq!(
        h.ctl(ADDR_PATTERN_TRANSITION_MODE),
        TransitionMode::SysTime as u16
    );
    assert_eq!(h.ctl(ADDR_PATTERN_TRANSITION_VALUE_0), 0xCDEF);
    assert_eq!(h.ctl(ADDR_PATTERN_TRANSITION_VALUE_0 + 1), 0x89AB);
    assert_eq!(h.ctl(ADDR_PATTERN_TRANSITION_VALUE_0 + 2), 0x4567);
    assert_eq!(h.ctl(ADDR_PATTERN_TRANSITION_VALUE_0 + 3), 0x0123);
}

#[test]
fn change_pattern_bank_rejects_invalid_bank() {
    let mut h = Harness::new();
    h.deliver(&change_pattern_bank(
        0,
        invalid_bank(),
        TransitionMode::Immediate,
        0,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);
}

#[test]
fn change_mod_bank_writes_transition_and_req_bank_and_latches() {
    let mut h = Harness::new();
    let latches_at_boot = h.latch_count(CTL_FLAG_MOD_SET);

    h.deliver(&change_mod_bank(0, 1, TransitionMode::Immediate, 0));

    assert_eq!(h.data(), 0);
    assert_eq!(
        h.ctl(ADDR_MOD_TRANSITION_MODE),
        TransitionMode::Immediate as u16
    );
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
    assert_eq!(h.latch_count(CTL_FLAG_MOD_SET), latches_at_boot + 1);
    assert_eq!(h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_MOD_SET, 0);
}

#[test]
fn change_mod_bank_rejects_invalid_bank() {
    let mut h = Harness::new();
    h.deliver(&change_mod_bank(
        0,
        invalid_bank(),
        TransitionMode::Immediate,
        0,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);
}

#[test]
fn change_mod_bank_rejects_timed_transition_on_infinite_loop() {
    let mut h = Harness::new();

    h.deliver(&change_mod_bank(0, 1, TransitionMode::SyncIdx, 0));
    assert_eq!(h.data(), Error::InvalidTransitionMode as u8);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

    h.deliver(&change_mod_bank(1, 1, TransitionMode::Ext, 0));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
}

#[test]
fn change_mod_bank_rejects_immediate_transition_on_finite_loop() {
    let mut h = Harness::new();
    h.deliver(&config_mod_rep(0, 1, 10, 100, 4));
    assert_eq!(h.data(), 0);

    h.deliver(&change_mod_bank(1, 1, TransitionMode::Immediate, 0));
    assert_eq!(h.data(), Error::InvalidTransitionMode as u8);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

    h.deliver(&change_mod_bank(2, 1, TransitionMode::Gpio, 1));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
}

#[test]
fn change_pattern_bank_rejects_timed_transition_on_infinite_loop() {
    let mut h = Harness::new();

    h.deliver(&change_pattern_bank(0, 1, TransitionMode::Gpio, 0));
    assert_eq!(h.data(), Error::InvalidTransitionMode as u8);
    assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);

    h.deliver(&change_pattern_bank(1, 1, TransitionMode::Immediate, 0));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
}

#[test]
fn change_pattern_bank_rejects_immediate_transition_on_finite_loop() {
    let mut h = Harness::new();
    h.deliver(&config_pattern_rep(
        0,
        1,
        EMISSION_TYPE_RAW,
        2,
        EMISSION_MAX_INDICES,
        0,
        0,
        4,
    ));
    assert_eq!(h.data(), 0);

    h.deliver(&change_pattern_bank(1, 1, TransitionMode::Ext, 0));
    assert_eq!(h.data(), Error::InvalidTransitionMode as u8);
    assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);

    h.deliver(&change_pattern_bank(2, 1, TransitionMode::SyncIdx, 0));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
}

#[test]
fn change_mod_bank_rejects_sys_time_transition_within_margin() {
    let mut h = Harness::new();
    h.deliver(&config_mod_rep(0, 1, 10, 100, 4));
    assert_eq!(h.data(), 0);
    h.port.dc_sys_time = 1_000_000_000;

    h.deliver(&change_mod_bank(
        1,
        1,
        TransitionMode::SysTime,
        1_000_000_000 + SYS_TIME_TRANSITION_MARGIN_NS - 1,
    ));
    assert_eq!(h.data(), Error::MissTransitionTime as u8);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

    h.deliver(&change_mod_bank(
        2,
        1,
        TransitionMode::SysTime,
        1_000_000_000 + SYS_TIME_TRANSITION_MARGIN_NS,
    ));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
}

#[test]
fn change_pattern_bank_rejects_sys_time_transition_within_margin() {
    let mut h = Harness::new();
    h.deliver(&config_pattern_rep(
        0,
        1,
        EMISSION_TYPE_RAW,
        2,
        EMISSION_MAX_INDICES,
        0,
        0,
        4,
    ));
    assert_eq!(h.data(), 0);
    h.port.dc_sys_time = 2_000_000_000;

    h.deliver(&change_pattern_bank(
        1,
        1,
        TransitionMode::SysTime,
        2_000_000_000,
    ));
    assert_eq!(h.data(), Error::MissTransitionTime as u8);
    assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);

    h.deliver(&change_pattern_bank(
        2,
        1,
        TransitionMode::SysTime,
        2_000_000_000 + SYS_TIME_TRANSITION_MARGIN_NS + 1,
    ));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
}
