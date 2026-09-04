use std::vec;

use crate::fifo::FIFO_DEPTH;
use crate::fpga::TransitionMode;
use crate::params::{
    ADDR_FPGA_STATE, ADDR_MOD_REQ_RD_BANK, ADDR_VERSION_NUM_MAJOR, NUM_TRANSDUCERS,
};
use zerocopy::FromZeros;

use crate::app::ReadTelemetryPayload;
use crate::proto::{
    AL_STATUS_CODE_SM_WATCHDOG, AL_STATUS_CODE_SYNC_ERROR, Cmd, Error, FAILSAFE_TICKS,
    OUTPUT_MASK_WORDS, Telemetry,
};
use crate::tests::builders::{change_mod_bank_with_margin, config_mod_rep, force_fan, output_mask};
use crate::tests::mock::{Frame, Harness};

fn read_telemetry(seq: u8, id: u8) -> Frame {
    let mut p = ReadTelemetryPayload::new_zeroed();
    p.counter_id = id;
    Frame::from_payload(seq, Cmd::ReadTelemetry, &p)
}

fn unmute(h: &mut Harness, seq: u8) {
    let mask = vec![true; NUM_TRANSDUCERS];
    h.deliver(&output_mask(seq, &mask));
    assert_eq!(h.output_mask(0), 0xFFFF);
}

fn assert_muted(h: &Harness) {
    for i in 0..OUTPUT_MASK_WORDS {
        assert_eq!(h.output_mask(i), 0);
    }
}

#[test]
fn failsafe_trips_after_error_persists() {
    let mut h = Harness::new();
    unmute(&mut h, 0);
    h.port.al_status_code = AL_STATUS_CODE_SYNC_ERROR;

    h.tick_1ms(u32::from(FAILSAFE_TICKS) - 1);
    assert_eq!(h.output_mask(0), 0xFFFF);
    assert_eq!(h.telemetry(Telemetry::Failsafe), 0);

    h.tick_1ms(1);
    assert_muted(&h);
    assert_eq!(h.telemetry(Telemetry::Failsafe), 1);

    h.tick_1ms(u32::from(FAILSAFE_TICKS));
    assert_eq!(h.telemetry(Telemetry::Failsafe), 1);
}

#[test]
fn failsafe_trips_on_sm_watchdog() {
    let mut h = Harness::new();
    unmute(&mut h, 0);
    h.port.al_status_code = AL_STATUS_CODE_SM_WATCHDOG;

    h.tick_1ms(u32::from(FAILSAFE_TICKS));
    assert_muted(&h);
    assert_eq!(h.telemetry(Telemetry::Failsafe), 1);
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
    assert_eq!(h.telemetry(Telemetry::Failsafe), 0);

    h.tick_1ms(1);
    assert_muted(&h);
    assert_eq!(h.telemetry(Telemetry::Failsafe), 1);
}

#[test]
fn telemetry_counts_processed_frames() {
    let mut h = Harness::new();
    h.deliver(&Frame::new(0, Cmd::Nop));
    h.deliver(&Frame::new(1, Cmd::Nop));
    assert_eq!(h.telemetry(Telemetry::Processed), 2);
}

#[test]
fn telemetry_counts_dedup_hits() {
    let mut h = Harness::new();
    let f = Frame::new(0, Cmd::Nop);
    h.deliver(&f);
    h.deliver(&f);
    assert_eq!(h.telemetry(Telemetry::Dedup), 1);
    assert_eq!(h.telemetry(Telemetry::Processed), 1);
}

#[test]
fn telemetry_counts_seq_mismatch() {
    let mut h = Harness::new();
    h.deliver(&Frame::new(5, Cmd::Nop));
    assert_eq!(h.telemetry(Telemetry::SeqMismatch), 1);
    assert_eq!(h.telemetry(Telemetry::Processed), 0);
}

#[test]
fn telemetry_counts_dispatch_errors() {
    let mut h = Harness::new();
    h.deliver(&force_fan(0, 2));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    assert_eq!(h.telemetry(Telemetry::DispatchError), 1);
}

#[test]
fn telemetry_counts_fifo_drops() {
    let mut h = Harness::new();

    let capacity = u8::try_from(FIFO_DEPTH - 1).unwrap();
    for i in 0..=capacity {
        h.deliver_no_drain(&Frame::new(i, Cmd::Nop));
    }
    assert_eq!(h.telemetry(Telemetry::FifoDrop), 1);
}

#[test]
fn read_telemetry_returns_counter_value() {
    let mut h = Harness::new();
    h.deliver(&force_fan(0, 2));

    h.deliver(&read_telemetry(1, Telemetry::DispatchError as u8));
    assert_eq!(h.data(), 1);
}

#[test]
fn read_telemetry_rejects_unknown_counter() {
    let mut h = Harness::new();
    h.deliver(&read_telemetry(0, 0xFF));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
}

#[test]
fn read_telemetry_sync_resync_returns_fpga_state_high_byte() {
    let mut h = Harness::new();
    h.set_ctl(ADDR_FPGA_STATE, 0x2A83);
    h.deliver(&read_telemetry(0, Telemetry::SyncResync as u8));
    assert_eq!(h.data(), 0x2A);
}

#[test]
fn derived_telemetry_id_is_not_a_cpu_counter() {
    let h = Harness::new();
    for &id in Telemetry::ALL {
        assert_eq!(h.telemetry(id), 0);
    }
    assert_eq!(h.telemetry(Telemetry::SyncResync), 0);
}

#[test]
fn clear_resets_telemetry_counters() {
    let mut h = Harness::new();
    h.deliver(&force_fan(0, 2));
    assert_eq!(h.telemetry(Telemetry::DispatchError), 1);

    h.deliver(&Frame::new(1, Cmd::Clear));
    assert_eq!(h.telemetry(Telemetry::DispatchError), 0);
}

#[test]
fn read_fpga_functions_returns_version_register_high_byte() {
    let mut h = Harness::new();
    h.set_ctl(ADDR_VERSION_NUM_MAJOR, 0xA50B);

    h.deliver(&Frame::new(0, Cmd::ReadFpgaFunctions));
    assert_eq!(h.data(), 0xA5);
}

#[test]
fn change_bank_margin_overrides_default() {
    let mut h = Harness::new();
    h.deliver(&config_mod_rep(0, 1, 10, 100, 4));
    h.port.dc_sys_time = 1_000_000_000;

    h.deliver(&change_mod_bank_with_margin(
        1,
        1,
        TransitionMode::SysTime,
        1_000_000_000 + 999_999,
        1_000_000,
    ));
    assert_eq!(h.data(), Error::MissTransitionTime as u8);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

    h.deliver(&change_mod_bank_with_margin(
        2,
        1,
        TransitionMode::SysTime,
        1_000_000_000 + 1_000_000,
        1_000_000,
    ));
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
}
