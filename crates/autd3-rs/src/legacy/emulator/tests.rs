use autd3_rs_core::link::Link;
use autd3_rs_core::value::{Emission, Intensity, Phase};

use crate::legacy::emulator::{LegacyAudit, LegacyDevice, StmKind};
use crate::legacy::error::{
    INVALID_GAIN_STM_MODE, INVALID_INFO_TYPE, INVALID_MSG_ID, INVALID_SEGMENT_TRANSITION,
    INVALID_SILENCER_SETTINGS, INVALID_TRANSITION_MODE, MISS_TRANSITION_TIME, NO_ERROR,
    NOT_SUPPORTED_TAG,
};
use crate::legacy::wire::params::{
    CPU_VERSION_V12_1, GAIN_FLAG_UPDATE, GAIN_STM_FLAG_BEGIN, GAIN_STM_FLAG_END,
    GAIN_STM_FLAG_SEGMENT, MODULATION_FLAG_BEGIN, MODULATION_FLAG_END, MODULATION_FLAG_SEGMENT,
    REP_INFINITE, SILENCER_FLAG_STRICT_MODE, TRANSITION_MODE_IMMEDIATE, TRANSITION_MODE_NONE,
    TRANSITION_MODE_SYNC_IDX, TRANSITION_MODE_SYS_TIME,
};
use crate::legacy::wire::{
    FpgaState, InfoType, RX_FRAME_BYTES, RxFrame, Segment, TX_FRAME_BYTES, Tag, TxFrame,
};

const NUM_TRANSDUCERS: usize = 4;
const CYCLE_PERIOD_NS: u64 = 1_000_000;
const BASE_NS: u64 = 1_000_000_000_000;
const AHEAD_NS: u64 = 20_000_000;

struct Harness {
    device: LegacyDevice,
    msg_id: u8,
}

impl Harness {
    fn new() -> Self {
        let mut device = LegacyDevice::new(0, NUM_TRANSDUCERS);
        device.set_dc_sys_time(BASE_NS);
        Self { device, msg_id: 0 }
    }

    fn cycle(&mut self, tx: &[u8; TX_FRAME_BYTES]) -> RxFrame {
        let mut rx = [0u8; RX_FRAME_BYTES];
        self.device.cycle(tx, &mut rx);
        RxFrame::parse(rx)
    }

    fn idle(&mut self) -> RxFrame {
        let mut frame = TxFrame::new();
        frame.header.msg_id = self.last_msg_id();
        self.cycle(&frame.to_bytes())
    }

    fn last_msg_id(&self) -> u8 {
        (self.msg_id + 0x0F) & 0x0F
    }

    fn send_with_msg_id(&mut self, msg_id: u8, slot_1: &[u8], slot_2: Option<&[u8]>) -> u8 {
        let mut frame = TxFrame::new();
        frame.header.msg_id = msg_id;
        frame.payload[..slot_1.len()].copy_from_slice(slot_1);
        if let Some(slot_2) = slot_2 {
            let offset = slot_1.len().next_multiple_of(2);
            frame.header.slot_2_offset = u16::try_from(offset).unwrap();
            frame.payload[offset..offset + slot_2.len()].copy_from_slice(slot_2);
        }
        self.cycle(&frame.to_bytes());
        self.device.err()
    }

    fn send_frame(&mut self, slot_1: &[u8], slot_2: Option<&[u8]>) -> u8 {
        let msg_id = self.msg_id;
        self.msg_id = (self.msg_id + 1) & 0x0F;
        self.send_with_msg_id(msg_id, slot_1, slot_2)
    }

    fn send(&mut self, payload: &[u8]) -> u8 {
        self.send_frame(payload, None)
    }
}

fn gain(segment: u8, flag: u8) -> Vec<u8> {
    let mut payload = vec![Tag::Gain.as_u8(), segment, flag, 0];
    payload.extend_from_slice(&[0u8; NUM_TRANSDUCERS * 2]);
    payload
}

fn gain_stm(
    flag: u8,
    mode: u8,
    transition_mode: u8,
    freq_div: u16,
    rep: u16,
    value: u64,
) -> Vec<u8> {
    let mut payload = vec![Tag::GainStm.as_u8(), flag, mode, transition_mode];
    payload.extend_from_slice(&freq_div.to_le_bytes());
    payload.extend_from_slice(&rep.to_le_bytes());
    payload.extend_from_slice(&value.to_le_bytes());
    payload.extend_from_slice(&[0u8; NUM_TRANSDUCERS * 2]);
    payload
}

fn modulation(flag: u8, transition_mode: u8, freq_div: u16, rep: u16, data: &[u8]) -> Vec<u8> {
    let mut payload = vec![
        Tag::Modulation.as_u8(),
        flag,
        u8::try_from(data.len()).unwrap(),
        transition_mode,
    ];
    payload.extend_from_slice(&freq_div.to_le_bytes());
    payload.extend_from_slice(&rep.to_le_bytes());
    payload.extend_from_slice(&0u64.to_le_bytes());
    payload.extend_from_slice(data);
    payload
}

fn change_bank(tag: Tag, segment: u8, transition_mode: u8, value: u64) -> Vec<u8> {
    let mut payload = vec![tag.as_u8(), segment, transition_mode, 0, 0, 0, 0, 0];
    payload.extend_from_slice(&value.to_le_bytes());
    payload
}

fn silencer(flag: u8, intensity: u16, phase: u16) -> Vec<u8> {
    let mut payload = vec![Tag::Silencer.as_u8(), flag];
    payload.extend_from_slice(&intensity.to_le_bytes());
    payload.extend_from_slice(&phase.to_le_bytes());
    payload
}

fn finite_modulation_in_bank_b() -> Vec<u8> {
    modulation(
        MODULATION_FLAG_BEGIN | MODULATION_FLAG_END | MODULATION_FLAG_SEGMENT,
        TRANSITION_MODE_NONE,
        0xFFFF,
        1,
        &[0xFF, 0xFF],
    )
}

#[test]
fn an_unknown_tag_is_rejected() {
    let mut harness = Harness::new();
    assert_eq!(harness.send(&[0xFE, 0x00]), NOT_SUPPORTED_TAG);
}

#[test]
fn a_msg_id_above_the_maximum_is_rejected() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send_with_msg_id(0x10, &[Tag::Nop.as_u8(), 0], None),
        INVALID_MSG_ID
    );
}

#[test]
fn an_unknown_firmware_info_type_is_rejected() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send(&[Tag::FirmInfo.as_u8(), 0x07]),
        INVALID_INFO_TYPE
    );
}

#[test]
fn a_gain_stm_mode_above_phase_half_is_rejected() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send(&gain_stm(
            GAIN_STM_FLAG_BEGIN,
            3,
            TRANSITION_MODE_IMMEDIATE,
            0xFFFF,
            REP_INFINITE,
            0
        )),
        INVALID_GAIN_STM_MODE
    );
}

#[test]
fn a_gain_stm_bank_switch_onto_a_single_pattern_bank_is_rejected() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send(&change_bank(
            Tag::GainStmLegacyChangePatternBank,
            0,
            TRANSITION_MODE_IMMEDIATE,
            0
        )),
        INVALID_SEGMENT_TRANSITION
    );
}

#[test]
fn a_foci_stm_bank_switch_onto_a_gain_bank_is_rejected() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send(&change_bank(
            Tag::FociStmLegacyChangePatternBank,
            0,
            TRANSITION_MODE_IMMEDIATE,
            0
        )),
        INVALID_SEGMENT_TRANSITION
    );
}

#[test]
fn a_sys_time_transition_that_is_far_enough_ahead_is_accepted() {
    let mut harness = Harness::new();
    assert_eq!(harness.send(&finite_modulation_in_bank_b()), NO_ERROR);
    assert_eq!(
        harness.send(&change_bank(
            Tag::ModulationLegacyChangePatternBank,
            1,
            TRANSITION_MODE_SYS_TIME,
            BASE_NS + AHEAD_NS
        )),
        NO_ERROR
    );
}

#[test]
fn the_same_transition_is_missed_once_the_emulated_clock_has_advanced() {
    let mut harness = Harness::new();
    assert_eq!(harness.send(&finite_modulation_in_bank_b()), NO_ERROR);
    for _ in 0..AHEAD_NS / CYCLE_PERIOD_NS {
        harness.idle();
    }
    assert_eq!(
        harness.send(&change_bank(
            Tag::ModulationLegacyChangePatternBank,
            1,
            TRANSITION_MODE_SYS_TIME,
            BASE_NS + AHEAD_NS
        )),
        MISS_TRANSITION_TIME
    );
}

#[test]
fn a_frozen_clock_never_misses_a_transition() {
    let mut harness = Harness::new();
    harness.device.set_cycle_period_ns(0);
    assert_eq!(harness.send(&finite_modulation_in_bank_b()), NO_ERROR);
    for _ in 0..AHEAD_NS / CYCLE_PERIOD_NS {
        harness.idle();
    }
    assert_eq!(harness.device.dc_sys_time_ns(), BASE_NS);
    assert_eq!(
        harness.send(&change_bank(
            Tag::ModulationLegacyChangePatternBank,
            1,
            TRANSITION_MODE_SYS_TIME,
            BASE_NS + AHEAD_NS
        )),
        NO_ERROR
    );
}

#[test]
fn the_emulated_clock_advances_one_cycle_period_per_cycle() {
    let mut harness = Harness::new();
    harness.idle();
    assert_eq!(harness.device.dc_sys_time_ns(), BASE_NS + CYCLE_PERIOD_NS);
    for _ in 0..9 {
        harness.idle();
    }
    assert_eq!(
        harness.device.dc_sys_time_ns(),
        BASE_NS + 10 * CYCLE_PERIOD_NS
    );
}

#[test]
fn a_silencer_slower_than_the_modulation_sampling_period_is_rejected() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send(&modulation(
            MODULATION_FLAG_BEGIN | MODULATION_FLAG_END,
            TRANSITION_MODE_NONE,
            5,
            REP_INFINITE,
            &[0xFF, 0xFF]
        )),
        INVALID_SILENCER_SETTINGS
    );
}

#[test]
fn a_rejected_silencer_leaves_the_previous_settings_in_place() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send(&gain_stm(
            GAIN_STM_FLAG_BEGIN,
            0,
            TRANSITION_MODE_NONE,
            50,
            REP_INFINITE,
            0
        )),
        NO_ERROR
    );
    assert_eq!(
        harness.send(&silencer(SILENCER_FLAG_STRICT_MODE, 100, 40)),
        INVALID_SILENCER_SETTINGS
    );
    assert_eq!(harness.device.silencer_completion_steps(), (10, 40));
    assert!(harness.device.silencer_strict());

    assert_eq!(harness.send(&silencer(0, 100, 40)), NO_ERROR);
    assert_eq!(harness.device.silencer_completion_steps(), (100, 40));
    assert!(!harness.device.silencer_strict());
}

#[test]
fn a_sampling_synced_transition_within_the_active_bank_is_rejected() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send(&modulation(
            MODULATION_FLAG_BEGIN | MODULATION_FLAG_END,
            TRANSITION_MODE_SYNC_IDX,
            0xFFFF,
            REP_INFINITE,
            &[0xFF, 0xFF]
        )),
        INVALID_TRANSITION_MODE
    );
}

#[test]
fn an_immediate_transition_onto_a_finite_bank_is_rejected() {
    let mut harness = Harness::new();
    assert_eq!(harness.send(&finite_modulation_in_bank_b()), NO_ERROR);
    assert_eq!(
        harness.send(&change_bank(
            Tag::ModulationLegacyChangePatternBank,
            1,
            TRANSITION_MODE_IMMEDIATE,
            0
        )),
        INVALID_TRANSITION_MODE
    );
}

#[test]
fn a_clear_does_not_stop_the_fpga_state_reads() {
    let mut harness = Harness::new();
    assert_eq!(harness.send(&[Tag::ReadsFpgaState.as_u8(), 1]), NO_ERROR);
    assert!(harness.device.reads_fpga_state());

    assert_eq!(harness.send(&[Tag::Clear.as_u8(), 0]), NO_ERROR);
    assert!(harness.device.reads_fpga_state());

    harness.idle();
    assert!(FpgaState(harness.idle().data).is_valid());
}

#[test]
fn the_fpga_state_is_republished_every_cycle_not_only_on_a_new_msg_id() {
    let mut harness = Harness::new();
    assert_eq!(harness.send(&[Tag::ReadsFpgaState.as_u8(), 1]), NO_ERROR);
    harness.idle();
    assert!(FpgaState(harness.idle().data).is_valid());

    harness.device.set_thermal_assert(true);
    harness.idle();
    assert!(FpgaState(harness.idle().data).is_thermal_assert());
}

#[test]
fn a_firmware_info_read_pauses_the_state_reads_without_disabling_them() {
    let mut harness = Harness::new();
    assert_eq!(harness.send(&[Tag::ReadsFpgaState.as_u8(), 1]), NO_ERROR);
    assert_eq!(
        harness.send(&[Tag::FirmInfo.as_u8(), InfoType::CpuMajor.as_u8()]),
        NO_ERROR
    );
    assert_eq!(harness.idle().data, CPU_VERSION_V12_1);
    assert!(harness.device.reads_fpga_state());

    assert_eq!(
        harness.send(&[Tag::FirmInfo.as_u8(), InfoType::Clear.as_u8()]),
        NO_ERROR
    );
    harness.idle();
    assert!(FpgaState(harness.idle().data).is_valid());
}

#[test]
fn a_bank_index_above_one_is_recorded_instead_of_silently_wrapped() {
    let mut harness = Harness::new();
    assert!(!harness.device.segment_out_of_range());
    assert_eq!(harness.send(&gain(2, 0)), NO_ERROR);
    assert!(harness.device.segment_out_of_range());
    assert_eq!(harness.device.current_stm_segment(), Segment::S0);
}

#[test]
fn writing_a_gain_keeps_the_previous_transition_value() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send(&gain_stm(
            GAIN_STM_FLAG_BEGIN,
            0,
            TRANSITION_MODE_NONE,
            0xFFFF,
            REP_INFINITE,
            0xDEAD_BEEF
        )),
        NO_ERROR
    );
    assert_eq!(
        harness.device.stm_transition(),
        (TRANSITION_MODE_NONE, 0xDEAD_BEEF)
    );

    assert_eq!(harness.send(&gain(0, GAIN_FLAG_UPDATE)), NO_ERROR);
    assert_eq!(
        harness.device.stm_transition(),
        (TRANSITION_MODE_SYNC_IDX, 0xDEAD_BEEF)
    );
}

#[test]
fn the_fan_flag_only_reaches_the_fpga_when_the_whole_frame_succeeds() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send_frame(&[Tag::ForceFan.as_u8(), 1], Some(&[0xFE, 0])),
        NOT_SUPPORTED_TAG
    );
    assert!(!harness.device.force_fan());

    assert_eq!(harness.send(&[Tag::Nop.as_u8(), 0]), NO_ERROR);
    assert!(harness.device.force_fan());
}

#[test]
fn the_emulated_gpio_in_flags_follow_the_same_commit_point() {
    let mut harness = Harness::new();
    assert_eq!(
        harness.send_frame(&[Tag::EmulateGpioIn.as_u8(), 0b0101], Some(&[0xFE, 0])),
        NOT_SUPPORTED_TAG
    );
    assert_eq!(harness.device.gpio_in(), [false; 4]);

    assert_eq!(harness.send(&[Tag::Nop.as_u8(), 0]), NO_ERROR);
    assert_eq!(harness.device.gpio_in(), [true, false, true, false]);
}

#[test]
fn the_cpu_gpio_out_tag_is_accepted_and_reset_by_a_clear() {
    let mut harness = Harness::new();
    assert_eq!(harness.send(&[Tag::CpuGpioOut.as_u8(), 0x5A]), NO_ERROR);
    assert_eq!(harness.device.cpu_gpio_out(), 0x5A);

    assert_eq!(harness.send(&[Tag::Clear.as_u8(), 0]), NO_ERROR);
    assert_eq!(harness.device.cpu_gpio_out(), 0);
}

#[test]
fn a_wedged_device_freezes_its_reply_and_its_clock() {
    let mut harness = Harness::new();
    assert_eq!(harness.send(&[Tag::ReadsFpgaState.as_u8(), 1]), NO_ERROR);
    harness.idle();
    let frozen = harness.idle();

    harness.device.wedge();
    for _ in 0..8 {
        assert_eq!(harness.send(&[Tag::Nop.as_u8(), 0]), NO_ERROR);
    }
    assert_eq!(harness.idle(), frozen);
    assert_eq!(
        harness.device.dc_sys_time_ns(),
        BASE_NS + 3 * CYCLE_PERIOD_NS
    );
}

#[test]
fn an_audit_link_keeps_one_independent_state_per_device() {
    let mut audit = LegacyAudit::new([NUM_TRANSDUCERS, NUM_TRANSDUCERS]);
    let devices = audit.devices();
    devices[1].with_mut(|d| d.set_thermal_assert(true));

    let mut frame = TxFrame::new();
    frame.header.msg_id = 0;
    let payload = gain_stm(
        GAIN_STM_FLAG_BEGIN | GAIN_STM_FLAG_END | GAIN_STM_FLAG_SEGMENT,
        0,
        TRANSITION_MODE_NONE,
        0xFFFF,
        REP_INFINITE,
        0,
    );
    frame.payload[..payload.len()].copy_from_slice(&payload);

    let tx = [frame.to_bytes(), frame.to_bytes()];
    let mut rx = [[0u8; RX_FRAME_BYTES]; 2];
    Link::cycle(&mut audit, &tx, &mut rx).unwrap();

    for device in &devices {
        device.with(|d| {
            assert_eq!(d.err(), NO_ERROR);
            assert_eq!(d.segment(Segment::S1).kind, StmKind::Gain);
            assert_eq!(d.segment(Segment::S1).cycle, 1);
            assert_eq!(
                d.segment(Segment::S1).emissions[0],
                vec![
                    Emission {
                        phase: Phase(0),
                        intensity: Intensity(0)
                    };
                    NUM_TRANSDUCERS
                ]
            );
        });
    }
    assert!(
        !devices[0]
            .with(LegacyDevice::fpga_state)
            .is_thermal_assert()
    );
    assert!(!devices[0].with(LegacyDevice::segment_out_of_range));
    assert!(
        devices[1]
            .with(LegacyDevice::fpga_state)
            .is_thermal_assert()
    );
}
