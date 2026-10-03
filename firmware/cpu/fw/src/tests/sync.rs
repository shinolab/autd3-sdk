use crate::params::{ADDR_CTL_FLAG, ADDR_MOD_CYCLE0, ADDR_SYNC_TIME_0, CTL_FLAG_SYNC_SET};
use crate::proto::{Cmd, Error, FRAME_BYTES_MAX, PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX};
use crate::tests::builders::{config_mod, write_foci_buffer, write_mod_buffer};
use crate::tests::mock::{Frame, Harness};

const _: () = assert!(FRAME_BYTES_MAX == 1448);
const _: () = assert!(FRAME_BYTES_MAX == 2 + PAYLOAD_BYTES);
const _: () = assert!(REPLY_DATA_BYTES_MAX == 32);

#[test]
fn synchronize_writes_next_sync_edge_in_sys_time_ticks_and_latches() {
    let mut h = Harness::new();
    h.port.next_sync_edge = 1_700_000_000_123_000_000;

    h.deliver(&Frame::new(0, Cmd::Synchronize));

    assert_eq!(h.ack(), 0);
    assert_eq!(h.status(), 0);
    assert_eq!(h.ctl(ADDR_SYNC_TIME_0), 0x7000);
    assert_eq!(h.ctl(ADDR_SYNC_TIME_0 + 1), 0xB0A6);
    assert_eq!(h.ctl(ADDR_SYNC_TIME_0 + 2), 0xB0F7);
    assert_eq!(h.ctl(ADDR_SYNC_TIME_0 + 3), 0x007B);
    assert_eq!(h.latch_count(CTL_FLAG_SYNC_SET), 1);
    assert_eq!(h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_SYNC_SET, 0);
}

#[test]
fn synchronize_returns_sync_not_ready_before_the_pulse_runs() {
    let mut h = Harness::new();
    h.port.next_sync_edge = 0;

    h.deliver(&Frame::new(0, Cmd::Synchronize));
    assert_eq!(h.status(), Error::SyncNotReady as u8);
    assert_eq!(h.ctl(ADDR_SYNC_TIME_0), 0);
    assert_eq!(h.latch_count(CTL_FLAG_SYNC_SET), 0);

    h.deliver(&Frame::new(1, Cmd::ReadErrorDetail));
    assert_eq!(h.reply_data(), [Error::SyncNotReady as u8]);
}

#[test]
fn set_and_wait_update_times_out_when_latch_stuck() {
    let mut h = Harness::new();
    h.port.latch_stuck = true;

    h.port.next_sync_edge = 0x1122_3344_5566_7788;
    h.deliver(&Frame::new(0, Cmd::Synchronize));
    assert_eq!(h.status(), Error::FpgaTimeout as u8);

    h.deliver(&Frame::new(1, Cmd::ReadErrorDetail));
    assert_eq!(h.reply_data(), [Error::FpgaTimeout as u8]);

    h.port.latch_stuck = false;
}

#[test]
fn fpga_init_latch_timeout_is_latched_into_error_detail() {
    let mut h = Harness::new();
    h.port.latch_stuck = true;
    h.init();
    h.port.latch_stuck = false;

    h.deliver(&Frame::new(0, Cmd::ReadErrorDetail));
    assert_eq!(h.reply_data(), [Error::FpgaTimeout as u8]);
}

#[test]
fn clear_reports_fpga_timeout_when_latch_stuck() {
    let mut h = Harness::new();
    h.port.latch_stuck = true;

    h.deliver(&Frame::new(0, Cmd::Clear));
    assert_eq!(h.status(), Error::FpgaTimeout as u8);

    h.port.latch_stuck = false;
}

#[test]
fn fpga_state_survives_reset() {
    let mut h = Harness::new();

    h.deliver(&write_foci_buffer(0, 0, 0, &[0x5A5A]));
    h.deliver(&write_mod_buffer(1, 1, 8, &[0x77]));
    h.deliver(&config_mod(2, 1, 5, 256));
    assert_eq!(h.status(), 0);

    h.deliver(&Frame::new(99, Cmd::Reset));
    assert_eq!(h.expected_seq(), 0);

    assert_eq!(h.emission_word(0, 0), 0x5A5A);
    assert_eq!(h.mod_word(1, 4), 0x0077);
    assert_eq!(h.ctl(ADDR_MOD_CYCLE0 + 1), 255);
}
