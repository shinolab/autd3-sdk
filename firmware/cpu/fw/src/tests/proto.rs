use zerocopy::FromZeros;
use zerocopy::little_endian::U16;

use crate::cmd::xor_hash::{XOR_HASH_MAX_DATA_LEN, XorHashPayload};
use crate::proto::{Cmd, Error};
use crate::tests::builders::{xor_hash_bad, xor_hash_corrupted, xor_hash_ok};
use crate::tests::mock::{Frame, Harness};

#[test]
fn initial_ack_is_sentinel_byte() {
    let h = Harness::new();
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);
}

#[test]
fn matching_seq_advances_ack_and_expected_seq() {
    let mut h = Harness::new();
    h.cpu.set_fw_version(0xAB, 0x12, 0x34);
    h.cpu.set_error_detail(Error::MissTransitionTime);

    h.deliver(&xor_hash_ok(0, 0, &[0x01, 0x02, 0x04]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
    assert_eq!(h.data(), 0);

    h.deliver(&Frame::new(1, Cmd::ReadCpuFwVersionMajor));
    assert_eq!(h.ack(), 1);
    assert_eq!(h.expected_seq(), 2);
    assert_eq!(h.data(), 0xAB);

    h.deliver(&Frame::new(2, Cmd::ReadCpuFwVersionMinor));
    assert_eq!(h.ack(), 2);
    assert_eq!(h.expected_seq(), 3);
    assert_eq!(h.data(), 0x12);

    h.deliver(&Frame::new(3, Cmd::ReadCpuFwVersionPatch));
    assert_eq!(h.ack(), 3);
    assert_eq!(h.expected_seq(), 4);
    assert_eq!(h.data(), 0x34);

    h.deliver(&Frame::new(4, Cmd::ReadErrorDetail));
    assert_eq!(h.ack(), 4);
    assert_eq!(h.expected_seq(), 5);
    assert_eq!(h.data(), Error::MissTransitionTime as u8);
}

#[test]
fn mismatched_seq_is_dropped() {
    let mut h = Harness::new();
    h.deliver(&xor_hash_ok(5, 0, &[0xAA]));
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);
}

#[test]
fn xor_hash_with_non_zero_xor_returns_err_invalid_data() {
    let mut h = Harness::new();
    h.deliver(&xor_hash_bad(0, &[0xAA]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.data(), Error::InvalidData as u8);
    h.deliver(&Frame::new(1, Cmd::ReadErrorDetail));
    assert_eq!(h.data(), Error::InvalidData as u8);
}

#[test]
fn unknown_cmd_sets_error_detail() {
    let mut h = Harness::new();
    h.deliver(&Frame::raw(0, 0x7F));
    assert_eq!(h.data(), Error::UnknownCmd as u8);
    h.deliver(&Frame::new(1, Cmd::ReadErrorDetail));
    assert_eq!(h.data(), Error::UnknownCmd as u8);
}

#[test]
fn duplicate_frame_is_suppressed_at_isr_boundary() {
    let mut h = Harness::new();
    let f = xor_hash_ok(0, 0, &[0x42]);
    h.deliver(&f);
    h.deliver(&f);
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn reset_during_in_flight_drain_overrides_stale_frame() {
    let mut h = Harness::new();

    let stale = xor_hash_corrupted(0, 1, &[0x5A]);
    h.deliver_no_drain(&stale);
    h.arm_isr_reset();

    assert!(h.process_one());
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.data(), 0);
    assert_eq!(h.expected_seq(), 0);

    assert!(!h.process_one());

    h.deliver(&xor_hash_ok(0, 0, &[0x11]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn reset_returns_proto_state_to_post_boot_baseline() {
    let mut h = Harness::new();
    h.cpu.set_fw_version(0x42, 0x05, 0x99);
    h.cpu.set_error_detail(Error::SyncNotReady);

    h.deliver(&xor_hash_ok(0, 0, &[]));
    h.deliver(&xor_hash_ok(1, 0, &[]));
    assert_eq!(h.ack(), 1);
    assert_eq!(h.expected_seq(), 2);

    h.deliver(&Frame::new(99, Cmd::Reset));
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);

    h.deliver(&Frame::new(0, Cmd::ReadCpuFwVersionMajor));
    assert_eq!(h.data(), 0x42);
    h.deliver(&Frame::new(1, Cmd::ReadCpuFwVersionMinor));
    assert_eq!(h.data(), 0x05);
    h.deliver(&Frame::new(2, Cmd::ReadCpuFwVersionPatch));
    assert_eq!(h.data(), 0x99);
    h.deliver(&Frame::new(3, Cmd::ReadErrorDetail));
    assert_eq!(h.data(), Error::SyncNotReady as u8);
}

#[test]
fn nop_acks_without_changing_state() {
    let mut h = Harness::new();
    h.cpu.set_error_detail(Error::FpgaTimeout);

    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.data(), 0);
    assert_eq!(h.expected_seq(), 1);

    h.deliver(&Frame::new(1, Cmd::ReadErrorDetail));
    assert_eq!(h.data(), Error::FpgaTimeout as u8);
}

#[test]
fn seq_wraparound_boundary() {
    let mut h = Harness::new();
    for i in 0..257u16 {
        h.deliver(&xor_hash_ok((i & 0xFF) as u8, 0, &[]));
    }
    assert_eq!(h.expected_seq(), 1);
    assert_eq!(h.ack(), 0);
}

#[test]
fn unknown_non_streaming_cmd_sets_error_detail() {
    let mut h = Harness::new();
    h.deliver(&Frame::raw(0, 0xEE));
    assert_eq!(h.data(), Error::UnknownCmd as u8);
}

#[test]
fn xor_hash_with_xor_zero_returns_success() {
    let mut h = Harness::new();
    h.port.total_sleep_ms = 0;
    h.deliver(&xor_hash_ok(0, 0, &[0x11, 0x22, 0x33]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.data(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn xor_hash_sleep_is_forwarded_to_port_hook() {
    let mut h = Harness::new();
    h.port.total_sleep_ms = 0;
    h.deliver(&xor_hash_ok(0, 7, &[0x01]));
    assert_eq!(h.data(), 0);
    assert_eq!(h.port.total_sleep_ms, 7);
}

#[test]
fn xor_hash_too_large_data_len_returns_err_invalid_payload() {
    let mut h = Harness::new();

    let mut p = XorHashPayload::new_zeroed();
    p.data_len = U16::new(u16::try_from(XOR_HASH_MAX_DATA_LEN + 1).unwrap());
    h.deliver(&Frame::from_payload(0, Cmd::XorHash, &p));

    assert_eq!(h.ack(), 0);
    assert_eq!(h.data(), Error::InvalidPayload as u8);
}

#[test]
fn xor_hash_empty_data_returns_success() {
    let mut h = Harness::new();
    h.deliver(&xor_hash_ok(0, 0, &[]));
    assert_eq!(h.data(), 0);
}

#[test]
fn consecutive_frames_each_process_immediately() {
    let mut h = Harness::new();

    h.deliver(&xor_hash_ok(0, 0, &[]));
    assert_eq!(h.ack(), 0);
    h.deliver(&xor_hash_ok(1, 0, &[]));
    assert_eq!(h.ack(), 1);
    h.deliver(&xor_hash_ok(2, 0, &[]));
    assert_eq!(h.ack(), 2);
    assert_eq!(h.expected_seq(), 3);
}

#[test]
fn same_seq_different_cmd_is_not_suppressed_at_isr_boundary() {
    let mut h = Harness::new();
    h.deliver(&Frame::new(0, Cmd::Reset));
    assert_eq!(h.expected_seq(), 0);

    h.deliver(&xor_hash_ok(0, 0, &[0xCD]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn dedup_state_resets_on_init_app() {
    let mut h = Harness::new();
    h.deliver(&xor_hash_ok(0, 0, &[]));
    assert_eq!(h.expected_seq(), 1);

    h.init();
    h.deliver(&xor_hash_ok(0, 0, &[]));
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn handshake_survives_worst_case_dedup_collision_after_crashed_client() {
    let mut h = Harness::new();
    h.deliver(&Frame::new(0, Cmd::Reset));

    h.deliver(&xor_hash_ok(0, 0, &[]));
    assert_eq!(h.expected_seq(), 1);

    h.deliver(&Frame::new(0, Cmd::Reset));
    h.deliver(&Frame::new(1, Cmd::Reset));

    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);

    h.deliver(&xor_hash_ok(0, 0, &[0x11, 0x22]));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.data(), 0);
    assert_eq!(h.expected_seq(), 1);
}
