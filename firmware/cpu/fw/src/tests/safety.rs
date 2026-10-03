use std::vec;

use crate::fifo::FIFO_DEPTH;
use crate::fpga::TransitionMode;
use crate::params::{
    ADDR_FPGA_STATE, ADDR_MOD_REQ_RD_BANK, ADDR_VERSION_NUM_MAJOR, NUM_TRANSDUCERS,
};
use crate::proto::{Cmd, Error, FAILSAFE_TIMEOUT_MS, OUTPUT_MASK_WORDS, Telemetry};
use crate::tests::builders::{
    activate_mod_bank_with_margin, config_mod_rep, force_fan, output_mask,
};
use crate::tests::mock::{Frame, Harness};

fn read_telemetry(h: &mut Harness, seq: u8) -> std::vec::Vec<u32> {
    h.deliver(&Frame::new(seq, Cmd::ReadTelemetry));
    assert_eq!(h.status(), 0);
    let data = h.reply_data();
    assert_eq!(data.len(), Telemetry::REPLY_BYTES);
    data.chunks(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
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
fn failsafe_trips_once_the_host_is_silent_for_the_timeout() {
    let mut h = Harness::new();
    unmute(&mut h, 0);

    h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS - 1);
    h.tick_1ms(1);
    assert_eq!(h.output_mask(0), 0xFFFF);
    assert_eq!(h.telemetry(Telemetry::Failsafe), 0);

    h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
    h.tick_1ms(1);
    assert_muted(&h);
    assert_eq!(h.telemetry(Telemetry::Failsafe), 1);

    h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS * 3);
    h.tick_1ms(10);
    assert_eq!(h.telemetry(Telemetry::Failsafe), 1);
}

#[test]
fn failsafe_never_trips_before_the_host_is_seen() {
    let mut h = Harness::new();
    unmute(&mut h, 0);
    h.port.host_idle_ms = None;

    h.tick_1ms(FAILSAFE_TIMEOUT_MS * 2);
    assert_eq!(h.output_mask(0), 0xFFFF);
    assert_eq!(h.telemetry(Telemetry::Failsafe), 0);
}

#[test]
fn failsafe_rearms_after_the_host_comes_back() {
    let mut h = Harness::new();
    unmute(&mut h, 0);

    h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
    h.tick_1ms(1);
    assert_eq!(h.telemetry(Telemetry::Failsafe), 1);

    h.port.host_idle_ms = Some(3);
    h.tick_1ms(1);
    unmute(&mut h, 1);
    assert_eq!(h.output_mask(0), 0xFFFF);

    h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS + 7);
    h.tick_1ms(1);
    assert_muted(&h);
    assert_eq!(h.telemetry(Telemetry::Failsafe), 2);
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
    assert_eq!(h.status(), Error::InvalidPayload as u8);
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
fn read_telemetry_returns_every_counter_at_once() {
    let mut h = Harness::new();
    h.deliver(&force_fan(0, 2));

    let counters = read_telemetry(&mut h, 1);
    assert_eq!(counters.len(), Telemetry::ALL.len());
    assert_eq!(counters[Telemetry::DispatchError as usize], 1);
    assert_eq!(counters[Telemetry::Processed as usize], 1);
    assert_eq!(counters[Telemetry::FifoDrop as usize], 0);
}

#[test]
fn read_telemetry_sync_resync_returns_fpga_state_high_byte() {
    let mut h = Harness::new();
    h.set_ctl(ADDR_FPGA_STATE, 0x2A83);
    let counters = read_telemetry(&mut h, 0);
    assert_eq!(counters[Telemetry::SyncResync as usize], 0x2A);
}

#[test]
fn telemetry_counters_are_wider_than_a_byte() {
    let mut h = Harness::new();
    let mut seq = 0u8;
    for _ in 0..300 {
        h.deliver(&Frame::new(seq, Cmd::Nop));
        seq = seq.wrapping_add(1);
    }
    let counters = read_telemetry(&mut h, seq);
    assert_eq!(counters[Telemetry::Processed as usize], 300);
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

    h.deliver(&Frame::new(0, Cmd::ReadFirmwareInfo));
    assert_eq!(h.firmware_info().fpga_functions, 0xA5);
}

#[test]
fn activate_bank_margin_overrides_default() {
    let mut h = Harness::new();
    h.deliver(&config_mod_rep(0, 1, 10, 100, 4));
    h.port.sys_time = 1_000_000_000;

    h.deliver(&activate_mod_bank_with_margin(
        1,
        1,
        TransitionMode::SysTime,
        1_000_000_000 + 999_999,
        1_000_000,
    ));
    assert_eq!(h.status(), Error::MissTransitionTime as u8);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

    h.deliver(&activate_mod_bank_with_margin(
        2,
        1,
        TransitionMode::SysTime,
        1_000_000_000 + 1_000_000,
        1_000_000,
    ));
    assert_eq!(h.status(), 0);
    assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
}
