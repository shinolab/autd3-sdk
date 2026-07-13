use crate::params::{
    ADDR_CTL_FLAG, ADDR_ECAT_SYNC_CYCLE_0, ADDR_ECAT_SYNC_CYCLE_1, ADDR_ECAT_SYNC_TIME_0,
    ADDR_MOD_CYCLE0, CTL_FLAG_SYNC_SET,
};
use crate::proto::{
    CMD_CLEAR, CMD_READ_ERROR_DETAIL, CMD_RESET, CMD_SYNCHRONIZE, ERR_FPGA_TIMEOUT,
    ERR_INVALID_SYNC0_CYCLE, ERR_SYNC_NOT_READY, PAYLOAD_BYTES, RX_FRAME_BYTES, TX_FRAME_BYTES,
    WIRE_RX_FRAME_BYTES, WIRE_RX_GAP_END, WIRE_RX_GAP_START,
};
use crate::tests::builders::{config_mod, write_mod_buffer, write_pattern_buffer};
use crate::tests::mock::{Frame, Harness};

#[test]
fn synchronize_writes_next_sync0_and_latches() {
    let mut h = Harness::new();
    h.port.next_sync0 = 0x1122_3344_5566_7788;
    h.port.sync0_cycle_ns = 1_000_000;

    h.deliver(&Frame::new(0, CMD_SYNCHRONIZE));

    assert_eq!(h.ack(), 0);
    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_TIME_0), 0x7788);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_TIME_0 + 1), 0x5566);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_TIME_0 + 2), 0x3344);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_TIME_0 + 3), 0x1122);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_CYCLE_0), 20480);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_CYCLE_1), 0);
    assert_eq!(h.latch_count(CTL_FLAG_SYNC_SET), 1);
    assert_eq!(h.ctl(ADDR_CTL_FLAG) & CTL_FLAG_SYNC_SET, 0);
}

#[test]
fn synchronize_returns_sync_not_ready_when_dc_unset() {
    let mut h = Harness::new();
    h.port.next_sync0 = 0;

    h.deliver(&Frame::new(0, CMD_SYNCHRONIZE));
    assert_eq!(h.data(), ERR_SYNC_NOT_READY);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_TIME_0), 0);
    assert_eq!(h.latch_count(CTL_FLAG_SYNC_SET), 0);

    h.deliver(&Frame::new(1, CMD_READ_ERROR_DETAIL));
    assert_eq!(h.data(), ERR_SYNC_NOT_READY);
}

#[test]
fn synchronize_writes_sync0_cycle_from_esc_register() {
    let mut h = Harness::new();
    h.port.next_sync0 = 0x10;
    h.port.sync0_cycle_ns = 2_000_000;

    h.deliver(&Frame::new(0, CMD_SYNCHRONIZE));

    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_CYCLE_0), 40960);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_CYCLE_1), 0);
    assert_eq!(h.latch_count(CTL_FLAG_SYNC_SET), 1);
}

#[test]
fn synchronize_rejects_single_shot_sync0() {
    let mut h = Harness::new();
    h.port.sync0_cycle_ns = 0;

    h.deliver(&Frame::new(0, CMD_SYNCHRONIZE));

    assert_eq!(h.data(), ERR_INVALID_SYNC0_CYCLE);
    assert_eq!(h.latch_count(CTL_FLAG_SYNC_SET), 0);
}

#[test]
fn synchronize_rejects_non_multiple_of500us() {
    let mut h = Harness::new();
    h.port.sync0_cycle_ns = 750_000;

    h.deliver(&Frame::new(0, CMD_SYNCHRONIZE));

    assert_eq!(h.data(), ERR_INVALID_SYNC0_CYCLE);
    assert_eq!(h.latch_count(CTL_FLAG_SYNC_SET), 0);
}

#[test]
fn synchronize_writes_both_cycle_words_for_large_cycle() {
    let mut h = Harness::new();
    h.port.next_sync0 = 0x10;
    h.port.sync0_cycle_ns = 3_500_000;

    h.deliver(&Frame::new(0, CMD_SYNCHRONIZE));

    assert_eq!(h.data(), 0);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_CYCLE_0), 0x1800);
    assert_eq!(h.ctl(ADDR_ECAT_SYNC_CYCLE_1), 0x1);
}

#[test]
fn set_and_wait_update_times_out_when_latch_stuck() {
    let mut h = Harness::new();
    h.port.latch_stuck = true;

    h.port.next_sync0 = 0x1122_3344_5566_7788;
    h.deliver(&Frame::new(0, CMD_SYNCHRONIZE));
    assert_eq!(h.data(), ERR_FPGA_TIMEOUT);

    h.deliver(&Frame::new(1, CMD_READ_ERROR_DETAIL));
    assert_eq!(h.data(), ERR_FPGA_TIMEOUT);

    h.port.latch_stuck = false;
}

#[test]
fn fpga_init_latch_timeout_is_latched_into_error_detail() {
    let mut h = Harness::new();
    h.port.latch_stuck = true;
    h.init();
    h.port.latch_stuck = false;

    h.deliver(&Frame::new(0, CMD_READ_ERROR_DETAIL));
    assert_eq!(h.data(), ERR_FPGA_TIMEOUT);
}

#[test]
fn clear_reports_fpga_timeout_when_latch_stuck() {
    let mut h = Harness::new();
    h.port.latch_stuck = true;

    h.deliver(&Frame::new(0, CMD_CLEAR));
    assert_eq!(h.data(), ERR_FPGA_TIMEOUT);

    h.port.latch_stuck = false;
}

#[test]
fn fpga_state_survives_reset() {
    let mut h = Harness::new();

    h.deliver(&write_pattern_buffer(0, 0, 0, &[0x5A5A]));
    h.deliver(&write_mod_buffer(1, 1, 8, &[0x77]));
    h.deliver(&config_mod(2, 1, 5, 256));
    assert_eq!(h.data(), 0);

    h.deliver(&Frame::new(99, CMD_RESET));
    assert_eq!(h.expected_seq(), 0);

    assert_eq!(h.emission_word(0, 0), 0x5A5A);
    assert_eq!(h.mod_word(1, 4), 0x0077);
    assert_eq!(h.ctl(ADDR_MOD_CYCLE0 + 1), 255);
}

#[test]
fn struct_sizes_match_spec() {
    assert_eq!(RX_FRAME_BYTES, 626);
    assert_eq!(RX_FRAME_BYTES, 2 + PAYLOAD_BYTES);
    assert_eq!(TX_FRAME_BYTES, 2);
    assert_eq!(WIRE_RX_FRAME_BYTES, 628);
    assert_eq!(WIRE_RX_FRAME_BYTES, RX_FRAME_BYTES + 2);
    assert_eq!(WIRE_RX_GAP_END - WIRE_RX_GAP_START, 2);
}
