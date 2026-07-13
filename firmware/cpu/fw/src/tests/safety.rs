use std::vec::Vec;

use crate::FIFO_DEPTH;
use crate::params::{ADDR_MOD_REQ_RD_BANK, ADDR_VERSION_NUM_MAJOR, TRANSITION_MODE_SYS_TIME};
use crate::proto::{
    AL_STATUS_CODE_SM_WATCHDOG, AL_STATUS_CODE_SYNC_ERROR, CMD_CLEAR, CMD_READ_FPGA_FUNCTIONS,
    CMD_READ_TELEMETRY, CMD_STOP, ERR_INVALID_PAYLOAD, ERR_MISS_TRANSITION_TIME, FAILSAFE_TICKS,
    OUTPUT_MASK_WORDS, READ_TELEMETRY_OFFSET_COUNTER_ID, TELEMETRY_COUNT, TELEMETRY_DEDUP,
    TELEMETRY_DISPATCH_ERROR, TELEMETRY_FAILSAFE, TELEMETRY_FIFO_DROP, TELEMETRY_PROCESSED,
    TELEMETRY_SEQ_MISMATCH,
};
use crate::tests::builders::{
    change_mod_bank, config_mod_rep, force_fan, output_mask, with_margin, xor_hash_ok,
};
use crate::tests::mock::{Frame, Harness};

fn read_telemetry(seq: u8, id: u8) -> Frame {
    let mut f = Frame::new(seq, CMD_READ_TELEMETRY);
    f.payload()[READ_TELEMETRY_OFFSET_COUNTER_ID] = id;
    f
}

fn unmute(h: &mut Harness, seq: u8) {
    let words: Vec<u16> = (0..OUTPUT_MASK_WORDS).map(|_| 0xFFFF).collect();
    h.deliver(&output_mask(seq, &words));
    assert_eq!(h.output_mask(0), 0xFFFF);
}

fn assert_muted(h: &Harness) {
    for i in 0..OUTPUT_MASK_WORDS {
        assert_eq!(h.output_mask(i), 0);
    }
}

#[test]
fn stop_mutes_output_and_bypasses_seq_check() {
    let mut h = Harness::new();
    unmute(&mut h, 0);

    h.deliver_no_drain(&Frame::new(0x77, CMD_STOP));

    assert_muted(&h);
    assert_eq!(h.ack(), 0x77);
    assert_eq!(h.data(), 0);
    assert_eq!(h.expected_seq(), 0x78);
}

#[test]
fn stop_is_processed_inline_and_flushes_queue_in_fifo_mode() {
    let mut h = Harness::new();
    unmute(&mut h, 0);

    h.deliver_no_drain(&xor_hash_ok(1, 0, &[]));
    h.deliver_no_drain(&xor_hash_ok(2, 0, &[]));

    h.deliver_no_drain(&Frame::new(3, CMD_STOP));
    assert_muted(&h);
    assert_eq!(h.ack(), 3);
    assert_eq!(h.expected_seq(), 4);

    assert!(!h.process_one());
    assert_eq!(h.expected_seq(), 4);

    h.deliver(&xor_hash_ok(4, 0, &[0x11]));
    assert_eq!(h.ack(), 4);
    assert_eq!(h.expected_seq(), 5);
}

#[test]
fn stop_during_drain_reapplies_mute_and_proto_state() {
    let mut h = Harness::new();
    unmute(&mut h, 0);

    h.deliver_no_drain(&xor_hash_ok(1, 1, &[0x5A]));
    h.arm_isr_frame(9, CMD_STOP);

    assert!(h.process_one());
    assert_muted(&h);
    assert_eq!(h.ack(), 9);
    assert_eq!(h.data(), 0);
    assert_eq!(h.expected_seq(), 10);

    assert!(!h.process_one());
}

#[test]
fn stop_keeps_pattern_state_and_allows_restart() {
    let mut h = Harness::new();
    unmute(&mut h, 0);
    h.deliver(&Frame::new(1, CMD_STOP));
    assert_muted(&h);

    let words: Vec<u16> = (0..OUTPUT_MASK_WORDS).map(|_| 0xFFFF).collect();
    h.deliver(&output_mask(2, &words));
    assert_eq!(h.output_mask(0), 0xFFFF);
}

#[test]
fn failsafe_trips_after_error_persists() {
    let mut h = Harness::new();
    unmute(&mut h, 0);
    h.port.al_status_code = AL_STATUS_CODE_SYNC_ERROR;

    h.tick_1ms(u32::from(FAILSAFE_TICKS) - 1);
    assert_eq!(h.output_mask(0), 0xFFFF);
    assert_eq!(h.telemetry(TELEMETRY_FAILSAFE), 0);

    h.tick_1ms(1);
    assert_muted(&h);
    assert_eq!(h.telemetry(TELEMETRY_FAILSAFE), 1);

    h.tick_1ms(u32::from(FAILSAFE_TICKS));
    assert_eq!(h.telemetry(TELEMETRY_FAILSAFE), 1);
}

#[test]
fn failsafe_trips_on_sm_watchdog() {
    let mut h = Harness::new();
    unmute(&mut h, 0);
    h.port.al_status_code = AL_STATUS_CODE_SM_WATCHDOG;

    h.tick_1ms(u32::from(FAILSAFE_TICKS));
    assert_muted(&h);
    assert_eq!(h.telemetry(TELEMETRY_FAILSAFE), 1);
}

#[test]
fn failsafe_counter_resets_when_error_clears() {
    let mut h = Harness::new();
    unmute(&mut h, 0);

    h.port.al_status_code = AL_STATUS_CODE_SYNC_ERROR;
    h.tick_1ms(u32::from(FAILSAFE_TICKS) - 1);

    h.port.al_status_code = 0;
    h.tick_1ms(1);

    h.port.al_status_code = AL_STATUS_CODE_SYNC_ERROR;
    h.tick_1ms(u32::from(FAILSAFE_TICKS) - 1);
    assert_eq!(h.output_mask(0), 0xFFFF);
    assert_eq!(h.telemetry(TELEMETRY_FAILSAFE), 0);

    h.tick_1ms(1);
    assert_muted(&h);
    assert_eq!(h.telemetry(TELEMETRY_FAILSAFE), 1);
}

#[test]
fn telemetry_counts_processed_frames() {
    let mut h = Harness::new();
    h.deliver(&xor_hash_ok(0, 0, &[]));
    h.deliver(&xor_hash_ok(1, 0, &[]));
    assert_eq!(h.telemetry(TELEMETRY_PROCESSED), 2);
}

#[test]
fn telemetry_counts_dedup_hits() {
    let mut h = Harness::new();
    let f = xor_hash_ok(0, 0, &[]);
    h.deliver(&f);
    h.deliver(&f);
    assert_eq!(h.telemetry(TELEMETRY_DEDUP), 1);
    assert_eq!(h.telemetry(TELEMETRY_PROCESSED), 1);
}

#[test]
fn telemetry_counts_seq_mismatch() {
    let mut h = Harness::new();
    h.deliver(&xor_hash_ok(5, 0, &[]));
    assert_eq!(h.telemetry(TELEMETRY_SEQ_MISMATCH), 1);
    assert_eq!(h.telemetry(TELEMETRY_PROCESSED), 0);
}

#[test]
fn telemetry_counts_dispatch_errors() {
    let mut h = Harness::new();
    h.deliver(&force_fan(0, 2));
    assert_eq!(h.data(), ERR_INVALID_PAYLOAD);
    assert_eq!(h.telemetry(TELEMETRY_DISPATCH_ERROR), 1);
}

#[test]
fn telemetry_counts_fifo_drops() {
    let mut h = Harness::new();

    let capacity = u8::try_from(FIFO_DEPTH - 1).unwrap();
    for i in 0..=capacity {
        h.deliver_no_drain(&xor_hash_ok(i, 0, &[]));
    }
    assert_eq!(h.telemetry(TELEMETRY_FIFO_DROP), 1);
}

#[test]
fn read_telemetry_returns_counter_value() {
    let mut h = Harness::new();
    h.deliver(&force_fan(0, 2));

    h.deliver(&read_telemetry(1, TELEMETRY_DISPATCH_ERROR));
    assert_eq!(h.data(), 1);
}

#[test]
fn read_telemetry_rejects_unknown_counter() {
    let mut h = Harness::new();
    h.deliver(&read_telemetry(0, TELEMETRY_COUNT as u8));
    assert_eq!(h.data(), ERR_INVALID_PAYLOAD);
}

#[test]
fn clear_resets_telemetry_counters() {
    let mut h = Harness::new();
    h.deliver(&force_fan(0, 2));
    assert_eq!(h.telemetry(TELEMETRY_DISPATCH_ERROR), 1);

    h.deliver(&Frame::new(1, CMD_CLEAR));
    assert_eq!(h.telemetry(TELEMETRY_DISPATCH_ERROR), 0);
}

#[test]
fn read_fpga_functions_returns_version_register_high_byte() {
    let mut h = Harness::new();
    h.set_ctl(ADDR_VERSION_NUM_MAJOR, 0xA50B);

    h.deliver(&Frame::new(0, CMD_READ_FPGA_FUNCTIONS));
    assert_eq!(h.data(), 0xA5);
}

#[test]
fn change_bank_margin_overrides_default() {
    let mut h = Harness::new();
    h.deliver(&config_mod_rep(0, 1, 10, 100, 4));
    h.port.dc_sys_time = 1_000_000_000;

    h.deliver(&with_margin(
        change_mod_bank(1, 1, TRANSITION_MODE_SYS_TIME, 1_000_000_000 + 999_999),
        1_000_000,
    ));
    assert_eq!(h.data(), ERR_MISS_TRANSITION_TIME);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

    h.deliver(&with_margin(
        change_mod_bank(2, 1, TRANSITION_MODE_SYS_TIME, 1_000_000_000 + 1_000_000),
        1_000_000,
    ));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
}
