use crate::cmd::silencer::SILENCER_FLAG_STRICT_MODE;
use crate::fpga::{PHASE_CORR_WORDS, PWE_TABLE_SIZE, REP_INFINITE, TransitionMode};
use crate::params::{
    ADDR_CTL_FLAG, ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, ADDR_MOD_REQ_RD_BANK,
    ADDR_PATTERN_CYCLE0, ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MODE0, ADDR_PATTERN_REP0,
    ADDR_SILENCER_COMPLETION_STEPS_INTENSITY, ADDR_SILENCER_COMPLETION_STEPS_PHASE,
    ADDR_SILENCER_FLAG, ADDR_SILENCER_UPDATE_RATE_INTENSITY, ADDR_SILENCER_UPDATE_RATE_PHASE,
    CTL_FLAG_DEBUG_SET, CTL_FLAG_MOD_SET, CTL_FLAG_PATTERN_SET, CTL_FLAG_SILENCER_SET,
    CTL_FLAG_SYNC_SET, EMISSION_TYPE_RAW, NUM_BANKS, NUM_TRANSDUCERS,
    SILENCER_FLAG_FIXED_UPDATE_RATE_MODE,
};
use crate::proto::{Cmd, Error, OUTPUT_MASK_WORDS};
use crate::tests::builders::{change_mod_bank, config_mod, config_pattern, set_silencer};
use crate::tests::mock::{Frame, Harness};

#[test]
fn set_silencer_fixed_completion_steps_writes_registers_and_latches() {
    let mut h = Harness::new();
    let latches_at_boot = h.latch_count(CTL_FLAG_SILENCER_SET);

    h.deliver(&set_silencer(0, SILENCER_FLAG_STRICT_MODE, 256, 256, 5, 7));

    assert_eq!(h.data(), 0);
    assert_eq!(
        h.ctl(ADDR_SILENCER_FLAG),
        u16::from(SILENCER_FLAG_STRICT_MODE)
    );
    assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 256);
    assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 256);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 5);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 7);
    assert_eq!(h.latch_count(CTL_FLAG_SILENCER_SET), latches_at_boot + 1);
    assert_eq!(h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_SILENCER_SET, 0);
}

#[test]
fn set_silencer_fixed_update_rate_writes_registers_and_latches() {
    let mut h = Harness::new();
    let latches_at_boot = h.latch_count(CTL_FLAG_SILENCER_SET);

    h.deliver(&set_silencer(
        0,
        SILENCER_FLAG_FIXED_UPDATE_RATE_MODE,
        8,
        16,
        10,
        40,
    ));

    assert_eq!(h.data(), 0);
    assert_eq!(
        h.ctl(ADDR_SILENCER_FLAG),
        u16::from(SILENCER_FLAG_FIXED_UPDATE_RATE_MODE)
    );
    assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 8);
    assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 16);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);
    assert_eq!(h.latch_count(CTL_FLAG_SILENCER_SET), latches_at_boot + 1);
}

#[test]
fn set_silencer_rejects_zero_completion_steps_in_steps_mode() {
    let mut h = Harness::new();

    h.deliver(&set_silencer(0, 0, 256, 256, 0, 7));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&set_silencer(1, 0, 256, 256, 5, 0));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);
}

#[test]
fn set_silencer_rejects_zero_update_rate_in_rate_mode() {
    let mut h = Harness::new();

    h.deliver(&set_silencer(
        0,
        SILENCER_FLAG_FIXED_UPDATE_RATE_MODE,
        0,
        16,
        10,
        40,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&set_silencer(
        1,
        SILENCER_FLAG_FIXED_UPDATE_RATE_MODE,
        8,
        0,
        10,
        40,
    ));
    assert_eq!(h.data(), Error::InvalidPayload as u8);

    assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 256);
    assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 256);
    assert_eq!(h.ctl(ADDR_SILENCER_FLAG), 0);
}

#[test]
fn set_silencer_steps_mode_ignores_zero_update_rate() {
    let mut h = Harness::new();

    h.deliver(&set_silencer(0, 0, 0, 0, 5, 7));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 0);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 5);
}

#[test]
fn strict_silencer_rejects_too_fast_mod_config() {
    let mut h = Harness::new();
    h.deliver(&set_silencer(
        0,
        SILENCER_FLAG_STRICT_MODE,
        256,
        256,
        10,
        40,
    ));
    assert_eq!(h.data(), 0);

    h.deliver(&config_mod(1, 0, 9, 100));
    assert_eq!(h.data(), Error::InvalidSilencerSetting as u8);
    assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0), 0xFFFF);

    h.deliver(&config_mod(2, 0, 10, 100));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0), 10);
}

#[test]
fn strict_silencer_rejects_too_fast_pattern_config() {
    let mut h = Harness::new();
    h.deliver(&set_silencer(
        0,
        SILENCER_FLAG_STRICT_MODE,
        256,
        256,
        10,
        40,
    ));
    assert_eq!(h.data(), 0);

    h.deliver(&config_pattern(1, 0, EMISSION_TYPE_RAW, 20, 1, 0, 0));
    assert_eq!(h.data(), Error::InvalidSilencerSetting as u8);
    assert_eq!(h.ctl(ADDR_PATTERN_FREQ_DIV0), 0xFFFF);

    h.deliver(&config_pattern(2, 0, EMISSION_TYPE_RAW, 40, 1, 0, 0));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_PATTERN_FREQ_DIV0), 40);
}

#[test]
fn non_strict_silencer_does_not_guard_sampling() {
    let mut h = Harness::new();
    h.deliver(&set_silencer(0, 0, 256, 256, 10, 40));
    assert_eq!(h.data(), 0);

    h.deliver(&config_mod(1, 0, 1, 100));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0), 1);
}

#[test]
fn strict_silencer_rejected_when_active_sampling_too_fast() {
    let mut h = Harness::new();
    h.deliver(&config_mod(0, 0, 5, 100));
    assert_eq!(h.data(), 0);

    h.deliver(&set_silencer(1, SILENCER_FLAG_STRICT_MODE, 256, 256, 8, 40));
    assert_eq!(h.data(), Error::InvalidSilencerSetting as u8);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
    assert_eq!(h.ctl(ADDR_SILENCER_FLAG), 0);
}

#[test]
fn fixed_update_rate_mode_releases_guard() {
    let mut h = Harness::new();
    h.deliver(&set_silencer(
        0,
        SILENCER_FLAG_STRICT_MODE,
        256,
        256,
        10,
        40,
    ));
    assert_eq!(h.data(), 0);

    h.deliver(&set_silencer(
        1,
        SILENCER_FLAG_FIXED_UPDATE_RATE_MODE,
        8,
        16,
        10,
        40,
    ));
    assert_eq!(h.data(), 0);

    h.deliver(&config_mod(2, 0, 1, 100));
    assert_eq!(h.data(), 0);
}

#[test]
fn strict_silencer_rejects_switch_to_too_fast_bank() {
    let mut h = Harness::new();
    h.deliver(&config_mod(0, 1, 5, 100));
    assert_eq!(h.data(), 0);

    h.deliver(&set_silencer(
        1,
        SILENCER_FLAG_STRICT_MODE,
        256,
        256,
        10,
        40,
    ));
    assert_eq!(h.data(), 0);

    h.deliver(&change_mod_bank(2, 1, TransitionMode::Immediate, 0));
    assert_eq!(h.data(), Error::InvalidSilencerSetting as u8);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);
}

#[test]
fn clear_releases_strict_silencer_guard() {
    let mut h = Harness::new();
    h.deliver(&set_silencer(
        0,
        SILENCER_FLAG_STRICT_MODE,
        256,
        256,
        10,
        40,
    ));
    assert_eq!(h.data(), 0);

    h.deliver(&config_mod(1, 0, 5, 100));
    assert_eq!(h.data(), Error::InvalidSilencerSetting as u8);

    h.deliver(&Frame::new(2, Cmd::Clear));
    assert_eq!(h.data(), 0);

    h.deliver(&config_mod(3, 0, 5, 100));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0), 5);
}

#[test]
fn clear_restores_silencer_and_bank_baseline() {
    let mut h = Harness::new();
    h.deliver(&set_silencer(
        0,
        SILENCER_FLAG_STRICT_MODE,
        256,
        256,
        20,
        30,
    ));
    assert_eq!(h.data(), 0);
    h.deliver(&config_mod(1, 1, 50, 100));
    assert_eq!(h.data(), 0);
    h.deliver(&change_mod_bank(2, 1, TransitionMode::Immediate, 0));
    assert_eq!(h.data(), 0);

    h.deliver(&Frame::new(3, Cmd::Clear));
    assert_eq!(h.data(), 0);

    assert_eq!(h.ctl(ADDR_SILENCER_FLAG), 0);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);
    for bank in 0..u16::try_from(NUM_BANKS).unwrap() {
        assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0 + bank), 0xFFFF);
        assert_eq!(h.ctl(ADDR_PATTERN_FREQ_DIV0 + bank), 0xFFFF);
    }
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);
}

#[test]
fn boot_brings_fpga_to_legacy_clear_baseline() {
    let h = Harness::new();

    assert_eq!(h.ctl(ADDR_SILENCER_FLAG), 0);
    assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 256);
    assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 256);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
    assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);

    for bank in 0..u8::try_from(NUM_BANKS).unwrap() {
        assert_eq!(h.ctl(ADDR_MOD_CYCLE0 + u16::from(bank)), 1);
        assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0 + u16::from(bank)), 0xFFFF);
        assert_eq!(h.ctl(ADDR_MOD_REP0 + u16::from(bank)), REP_INFINITE);
        assert_eq!(h.mod_word(bank, 0), 0xFFFF);
    }

    for bank in 0..u8::try_from(NUM_BANKS).unwrap() {
        assert_eq!(
            h.ctl(ADDR_PATTERN_MODE0 + u16::from(bank)),
            u16::from(EMISSION_TYPE_RAW)
        );
        assert_eq!(h.ctl(ADDR_PATTERN_CYCLE0 + u16::from(bank)), 0);
        assert_eq!(h.ctl(ADDR_PATTERN_REP0 + u16::from(bank)), REP_INFINITE);
        assert_eq!(h.emission_word(bank, 0), 0);
        assert_eq!(h.emission_word(bank, NUM_TRANSDUCERS - 1), 0);
    }

    assert_eq!(h.port.phase_corr[0], 0);
    assert_eq!(h.port.phase_corr[PHASE_CORR_WORDS - 1], 0);
    assert_eq!(h.port.output_mask[0], 0xFFFF);
    assert_eq!(h.port.output_mask[OUTPUT_MASK_WORDS - 1], 0xFFFF);

    assert_eq!(h.port.pwe[0], 0x00);
    assert_eq!(h.port.pwe[1], 0x01);
    assert_eq!(h.port.pwe[128], 0x56);
    assert_eq!(h.port.pwe[PWE_TABLE_SIZE - 1], 0x100);

    assert_eq!(h.latch_count(CTL_FLAG_MOD_SET), 1);
    assert_eq!(h.latch_count(CTL_FLAG_PATTERN_SET), 1);
    assert_eq!(h.latch_count(CTL_FLAG_SILENCER_SET), 1);
    assert_eq!(h.latch_count(CTL_FLAG_DEBUG_SET), 1);
    assert_eq!(h.latch_count(CTL_FLAG_SYNC_SET), 0);
}
