use crate::proto::{Cmd, Error};
use crate::tests::builders::write_pattern_buffer;
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

    h.deliver(&Frame::new(0, Cmd::Nop));
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
    h.deliver(&Frame::new(5, Cmd::Nop));
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);
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
fn every_cmd_has_a_dispatch_arm() {
    for &cmd in Cmd::ALL {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, cmd));
        let next_seq = u8::from(cmd != Cmd::Reset);
        h.deliver(&Frame::new(next_seq, Cmd::ReadErrorDetail));
        assert_ne!(h.data(), Error::UnknownCmd as u8, "{cmd:?}");
    }
}

#[test]
fn duplicate_frame_is_suppressed_at_isr_boundary() {
    let mut h = Harness::new();
    let f = Frame::new(0, Cmd::Nop);
    h.deliver(&f);
    h.deliver(&f);
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn reset_during_inflight_drain_overrides_stale_frame() {
    let mut h = Harness::new();

    let stale = write_pattern_buffer(0, 0, 0, &[0x5A5A]);
    h.deliver_no_drain(&stale);
    h.arm_isr_reset();

    assert!(h.process_one());
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.data(), 0);
    assert_eq!(h.expected_seq(), 0);

    assert!(!h.process_one());

    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn reset_returns_proto_state_to_post_boot_baseline() {
    let mut h = Harness::new();
    h.cpu.set_fw_version(0x42, 0x05, 0x99);
    h.cpu.set_error_detail(Error::SyncNotReady);

    h.deliver(&Frame::new(0, Cmd::Nop));
    h.deliver(&Frame::new(1, Cmd::Nop));
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
        h.deliver(&Frame::new((i & 0xFF) as u8, Cmd::Nop));
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
fn consecutive_frames_each_process_immediately() {
    let mut h = Harness::new();

    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);
    h.deliver(&Frame::new(1, Cmd::Nop));
    assert_eq!(h.ack(), 1);
    h.deliver(&Frame::new(2, Cmd::Nop));
    assert_eq!(h.ack(), 2);
    assert_eq!(h.expected_seq(), 3);
}

#[test]
fn same_seq_different_cmd_is_not_suppressed_at_isr_boundary() {
    let mut h = Harness::new();
    h.deliver(&Frame::new(0, Cmd::Reset));
    assert_eq!(h.expected_seq(), 0);

    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn dedup_state_resets_on_init_app() {
    let mut h = Harness::new();
    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.expected_seq(), 1);

    h.init();
    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn handshake_survives_worst_case_dedup_collision_after_crashed_client() {
    let mut h = Harness::new();
    h.deliver(&Frame::new(0, Cmd::Reset));

    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.expected_seq(), 1);

    h.deliver(&Frame::new(0, Cmd::Reset));
    h.deliver(&Frame::new(1, Cmd::Reset));

    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);

    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.data(), 0);
    assert_eq!(h.expected_seq(), 1);
}
