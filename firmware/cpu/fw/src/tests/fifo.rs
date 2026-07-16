use crate::FIFO_DEPTH;
use zerocopy::FromZeros;

use crate::cmd::set_mode::SetModePayload;
use crate::proto::{Cmd, Error, Mode};
use crate::tests::builders::xor_hash_ok;
use crate::tests::mock::{Frame, Harness};

fn set_mode(seq: u8, mode: u8) -> Frame {
    let mut p = SetModePayload::new_zeroed();
    p.mode = mode;
    Frame::from_payload(seq, Cmd::SetMode, &p)
}

#[test]
fn default_mode_is_fifo() {
    let h = Harness::new();
    assert_eq!(h.cpu.mode(), Mode::Fifo);
}

#[test]
fn fifo_mode_defers_processing_until_drained() {
    let mut h = Harness::new();

    h.deliver_no_drain(&xor_hash_ok(0, 0, &[]));
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);

    h.cpu.process_pending(&mut h.port);
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn fifo_mode_drains_in_order() {
    let mut h = Harness::new();

    h.deliver_no_drain(&xor_hash_ok(0, 0, &[]));
    h.deliver_no_drain(&xor_hash_ok(1, 0, &[]));
    h.deliver_no_drain(&xor_hash_ok(2, 0, &[]));
    assert_eq!(h.expected_seq(), 0);

    h.cpu.process_pending(&mut h.port);
    assert_eq!(h.ack(), 2);
    assert_eq!(h.expected_seq(), 3);
}

#[test]
fn set_mode_low_latency_processes_frames_inline() {
    let mut h = Harness::new();

    h.deliver(&set_mode(0, Mode::LowLatency as u8));
    assert_eq!(h.cpu.mode(), Mode::LowLatency);
    assert_eq!(h.ack(), 0);

    h.deliver_no_drain(&xor_hash_ok(1, 0, &[]));
    assert_eq!(h.ack(), 1);
    assert_eq!(h.expected_seq(), 2);
}

#[test]
fn set_mode_rejects_unknown_mode() {
    let mut h = Harness::new();

    h.deliver(&set_mode(0, 0x02));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    assert_eq!(h.cpu.mode(), Mode::Fifo);
}

#[test]
fn reset_is_processed_inline_and_flushes_queue_in_fifo_mode() {
    let mut h = Harness::new();

    h.deliver_no_drain(&xor_hash_ok(0, 0, &[]));
    assert_eq!(h.expected_seq(), 0);

    h.deliver_no_drain(&Frame::new(0, Cmd::Reset));
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);

    h.cpu.process_pending(&mut h.port);
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);
}

#[test]
fn reset_flush_discards_queued_frames_mid_drain() {
    let mut h = Harness::new();

    h.deliver_no_drain(&xor_hash_ok(0, 0, &[]));
    h.deliver_no_drain(&xor_hash_ok(1, 0, &[]));
    h.deliver_no_drain(&xor_hash_ok(2, 0, &[]));
    assert_eq!(h.expected_seq(), 0);

    assert!(h.process_one());
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);

    h.deliver_no_drain(&Frame::new(0, Cmd::Reset));
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);

    assert!(!h.process_one());
    assert_eq!(h.expected_seq(), 0);

    h.deliver(&xor_hash_ok(0, 0, &[0x11]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn fifo_overflow_drops_beyond_capacity_and_accepts_after_drain() {
    let mut h = Harness::new();

    let capacity = u8::try_from(FIFO_DEPTH - 1).unwrap();
    for i in 0..capacity {
        h.deliver_no_drain(&xor_hash_ok(i, 0, &[]));
    }
    h.deliver_no_drain(&xor_hash_ok(capacity, 0, &[]));

    h.cpu.process_pending(&mut h.port);
    assert_eq!(h.ack(), capacity - 1);
    assert_eq!(h.expected_seq(), capacity);

    h.deliver(&xor_hash_ok(capacity, 0, &[]));
    assert_eq!(h.ack(), capacity);
    assert_eq!(h.expected_seq(), capacity + 1);
}

#[test]
fn xor_hash_sleep_accepted_in_fifo_mode() {
    let mut h = Harness::new();
    h.port.total_sleep_ms = 0;

    h.deliver(&xor_hash_ok(0, 5, &[0x01]));
    assert_eq!(h.data(), 0);
    assert_eq!(h.port.total_sleep_ms, 5);
}

#[test]
fn low_latency_defers_inline_while_fifo_non_empty() {
    let mut h = Harness::new();

    h.deliver_no_drain(&xor_hash_ok(0, 0, &[]));
    h.cpu.set_mode(Mode::LowLatency);

    h.deliver_no_drain(&xor_hash_ok(1, 0, &[]));
    assert_eq!(h.expected_seq(), 0);

    h.cpu.process_pending(&mut h.port);
    assert_eq!(h.ack(), 1);
    assert_eq!(h.expected_seq(), 2);

    h.deliver_no_drain(&xor_hash_ok(2, 0, &[]));
    assert_eq!(h.ack(), 2);
    assert_eq!(h.expected_seq(), 3);
}

#[test]
fn reset_inline_flushes_in_low_latency_mode() {
    let mut h = Harness::new();
    h.cpu.set_mode(Mode::LowLatency);

    h.deliver_no_drain(&xor_hash_ok(0, 0, &[]));
    h.deliver_no_drain(&xor_hash_ok(1, 0, &[]));
    assert_eq!(h.expected_seq(), 2);

    h.deliver_no_drain(&Frame::new(0, Cmd::Reset));
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);

    h.cpu.process_pending(&mut h.port);
    assert_eq!(h.expected_seq(), 0);

    h.deliver_no_drain(&xor_hash_ok(0, 0, &[0x22]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
}
