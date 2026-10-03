use crate::cmd::config_mod::ConfigModPayload;
use crate::cmd::force_fan::ForceFanPayload;
use crate::proto::{Cmd, Disposition, Error, FRAME_BYTES_MAX, Telemetry};
use crate::tests::builders::{config_mod, force_fan, write_foci_buffer};
use crate::tests::mock::{Frame, Harness};
use crate::version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};

#[test]
fn initial_ack_is_sentinel_byte() {
    let h = Harness::new();
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);
}

#[test]
fn matching_seq_advances_ack_and_expected_seq() {
    let mut h = Harness::new();

    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
    assert_eq!(h.status(), 0);

    h.deliver(&Frame::new(1, Cmd::ReadFirmwareInfo));
    assert_eq!(h.ack(), 1);
    assert_eq!(h.expected_seq(), 2);
    assert_eq!(h.status(), 0);
    assert_eq!(
        h.firmware_info().cpu_version,
        [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH]
    );
}

#[test]
fn mismatched_seq_is_dropped() {
    let mut h = Harness::new();
    h.deliver(&Frame::new(5, Cmd::Nop));
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);
}

#[test]
fn unknown_cmd_reports_unknown_cmd_and_advances_seq() {
    let mut h = Harness::new();
    h.deliver(&Frame::raw(0, 0x7F));
    assert_eq!(h.status(), Error::UnknownCmd as u8);
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
    assert_eq!(h.telemetry(Telemetry::DispatchError), 1);
}

#[test]
fn every_cmd_has_a_dispatch_arm() {
    for &cmd in Cmd::ALL {
        let mut h = Harness::new();
        h.deliver(&Frame::new(0, cmd));
        assert_ne!(h.status(), Error::UnknownCmd as u8, "{cmd:?}");
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

    let stale = write_foci_buffer(0, 0, 0, &[0x5A5A]);
    h.deliver_no_drain(&stale);
    h.arm_isr_reset();

    assert!(h.process_one());
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.status(), 0);
    assert_eq!(h.expected_seq(), 0);

    assert!(!h.process_one());

    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn reset_returns_proto_state_to_post_boot_baseline() {
    let mut h = Harness::new();

    h.deliver(&Frame::new(0, Cmd::Nop));
    h.deliver(&Frame::new(1, Cmd::Nop));
    assert_eq!(h.ack(), 1);
    assert_eq!(h.expected_seq(), 2);

    h.deliver(&Frame::new(99, Cmd::Reset));
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);

    h.deliver(&Frame::new(0, Cmd::ReadFirmwareInfo));
    assert_eq!(
        h.firmware_info().cpu_version,
        [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH]
    );
}

#[test]
fn nop_acks_without_changing_state() {
    let mut h = Harness::new();

    h.deliver(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);
    assert_eq!(h.status(), 0);
    assert_eq!(h.expected_seq(), 1);
    assert_eq!(h.telemetry(Telemetry::DispatchError), 0);
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
fn unknown_non_streaming_cmd_reports_unknown_cmd() {
    let mut h = Harness::new();
    h.deliver(&Frame::raw(0, 0xEE));
    assert_eq!(h.status(), Error::UnknownCmd as u8);
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
    assert_eq!(h.status(), 0);
    assert_eq!(h.expected_seq(), 1);
}

#[test]
fn payload_free_commands_reject_a_payload() {
    let mut h = Harness::new();
    let mut nop = Frame::new(0, Cmd::Nop);
    nop.set_payload_byte(0, 0);
    h.deliver(&nop);
    assert_eq!(h.ack(), 0);
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    let mut read = Frame::new(1, Cmd::ReadFirmwareInfo);
    read.set_payload_byte(0, 0);
    h.deliver(&read);
    assert_eq!(h.status(), Error::InvalidPayload as u8);
    assert_eq!(h.reply_data(), [] as [u8; 0]);
}

#[test]
fn fixed_length_payloads_must_match_exactly() {
    let mut h = Harness::new();
    let mut long = force_fan(0, 1);
    long.set_payload_byte(size_of::<ForceFanPayload>(), 0);
    h.deliver(&long);
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    let mut short = config_mod(1, 0, 10, 4);
    short.set_len(size_of::<ConfigModPayload>() - 1);
    h.deliver(&short);
    assert_eq!(h.status(), Error::InvalidPayload as u8);

    h.deliver(&force_fan(2, 1));
    assert_eq!(h.status(), 0);
}

#[test]
fn an_oversized_frame_is_dropped() {
    let mut h = Harness::new();
    let mut frame = std::vec![0u8; FRAME_BYTES_MAX + 1];
    frame[1] = Cmd::Nop as u8;
    assert_eq!(
        h.cpu.recv_frame(&mut h.port, &frame, 0),
        Disposition::Dropped
    );
    assert_eq!(h.ack(), 0xFF);
    assert_eq!(h.expected_seq(), 0);
}
