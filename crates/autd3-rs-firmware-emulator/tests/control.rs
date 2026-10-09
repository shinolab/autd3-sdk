mod common;

use autd3_cpu_wire::PatternBank;
use autd3_cpu_wire::cpu_params;
use autd3_cpu_wire::fpga_params::FpgaStateFlags;
use autd3_cpu_wire::payload::WritePatternRawPayload;
use autd3_rs_core::protocol::Cmd;
use autd3_rs_core::value::Intensity;
use autd3_rs_firmware_emulator::Device;
use autd3_rs_firmware_emulator::test_utils::FpgaEmulatorTestExt;
use zerocopy::IntoBytes;
use zerocopy::little_endian::U16;

use common::{NUM_TRANSDUCERS, frame};

#[test]
fn force_fan_toggles_and_survives_latch() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    device.send(&frame(0, Cmd::ForceFan, &[1]));
    assert!(device.fpga().force_fan());

    let silencer = [0u8; 10];
    device.send(&frame(1, Cmd::SetSilencer, &silencer));
    assert!(device.fpga().force_fan());

    device.send(&frame(2, Cmd::ForceFan, &[0]));
    assert!(!device.fpga().force_fan());
}

#[test]
fn gpio_out_writes_debug_values() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    let values = [
        0x0102_0304_0506_0708u64,
        0x1112_1314_1516_1718,
        0x2122_2324_2526_2728,
        0x3132_3334_3536_3738,
    ];
    let mut payload = [0u8; 32];
    for (i, v) in values.iter().enumerate() {
        payload[8 * i..][..8].copy_from_slice(&v.to_le_bytes());
    }
    device.send(&frame(0, Cmd::SetGpioOut, &payload));

    for (i, v) in values.iter().enumerate() {
        assert_eq!(device.fpga().gpio_out(i), *v);
    }
}

#[test]
fn pulse_width_encoder_overwrites_table() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    let mut payload = [0u8; 512];
    for i in 0..256 {
        let v = u16::try_from(255 - i).unwrap();
        payload[2 * i..][..2].copy_from_slice(&v.to_le_bytes());
    }
    device.send(&frame(0, Cmd::SetPulseWidthTable, &payload));

    assert_eq!(device.fpga().pulse_width_table(0), 255);
    assert_eq!(device.fpga().pulse_width_table(255), 0);
}

#[test]
fn phase_correction_applies_per_transducer() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    let mut payload = [0u8; NUM_TRANSDUCERS];
    for (i, b) in payload.iter_mut().enumerate() {
        *b = u8::try_from(i % 256).unwrap();
    }
    device.send(&frame(0, Cmd::SetPhaseCorrection, &payload));

    assert_eq!(device.fpga().phase_correction(0).0, 0);
    assert_eq!(device.fpga().phase_correction(1).0, 1);
    assert_eq!(
        device.fpga().phase_correction(NUM_TRANSDUCERS - 1).0,
        u8::try_from((NUM_TRANSDUCERS - 1) % 256).unwrap()
    );
}

#[test]
fn output_mask_disables_transducers() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    let mut payload = [0u8; 32];
    payload[0] = 0b0010_0001;
    device.send(&frame(0, Cmd::SetOutputMask, &payload));

    assert!(device.fpga().output_mask_enabled(0));
    assert!(!device.fpga().output_mask_enabled(1));
    assert!(device.fpga().output_mask_enabled(5));
    assert!(!device.fpga().output_mask_enabled(8));
}

#[test]
fn read_fpga_state_reports_thermal_and_default_banks() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    let pattern_mode = FpgaStateFlags::PATTERN_MODE.bits();
    let thermal = FpgaStateFlags::THERMAL_ASSERT.bits();

    let rx = device.send(&frame(0, Cmd::ReadFpgaState, &[]));
    assert_eq!(rx.data(), [pattern_mode]);

    device.fpga_mut().set_thermal(true);
    let rx = device.send(&frame(1, Cmd::ReadFpgaState, &[]));
    assert_eq!(rx.data(), [pattern_mode | thermal]);

    device.fpga_mut().set_thermal(false);
    let rx = device.send(&frame(2, Cmd::ReadFpgaState, &[]));
    assert_eq!(rx.data(), [pattern_mode]);
}

fn trip_failsafe(device: &mut Device) {
    let timeout = u32::try_from(cpu_params::FAILSAFE_TIMEOUT.as_millis()).unwrap();
    device.fpga_mut().set_host_idle_ms(Some(timeout));
    device.tick_1ms();
    device.fpga_mut().set_host_idle_ms(Some(0));
    device.tick_1ms();
}

fn write_full_intensity_pattern(device: &mut Device, seq: u8) {
    let header = WritePatternRawPayload {
        bank: PatternBank::B0,
        count: 1,
        index: U16::new(0),
    };
    let mut samples = [0u8; NUM_TRANSDUCERS * 2];
    samples[NUM_TRANSDUCERS..].fill(0xFF);
    let payload = [header.as_bytes(), &samples].concat();
    let rx = device.send(&frame(seq, Cmd::WritePatternRaw, &payload));
    assert_eq!(rx.status, autd3_cpu_wire::Error::None);
}

#[test]
fn the_failsafe_gates_the_output_and_keeps_the_output_mask() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    write_full_intensity_pattern(&mut device, 0);

    let mut mask = [0xFFu8; 32];
    mask[0] = 0b1111_1101;
    device.send(&frame(1, Cmd::SetOutputMask, &mask));
    let emitting = device.fpga().emissions().1;
    assert_eq!(emitting[0], Intensity::MAX);
    assert_eq!(emitting[1], Intensity::MIN);
    assert_eq!(emitting[2], Intensity::MAX);

    trip_failsafe(&mut device);
    assert!(device.fpga().failsafe());
    assert!(
        device
            .fpga()
            .emissions()
            .1
            .iter()
            .all(|&intensity| intensity == Intensity::MIN)
    );
    assert!(device.fpga().output_mask_enabled(0));
    assert!(!device.fpga().output_mask_enabled(1));
    let rx = device.send(&frame(2, Cmd::ReadFpgaState, &[]));
    assert_ne!(rx.data()[0] & FpgaStateFlags::FAILSAFE.bits(), 0);

    device.send(&frame(3, Cmd::SetOutputMask, &mask));
    assert!(device.fpga().failsafe());

    let rx = device.send(&frame(4, Cmd::ReleaseFailsafe, &[]));
    assert_eq!(rx.status, autd3_cpu_wire::Error::None);
    assert!(!device.fpga().failsafe());
    assert_eq!(device.fpga().emissions().1, emitting);
    let rx = device.send(&frame(5, Cmd::ReadFpgaState, &[]));
    assert_eq!(rx.data()[0] & FpgaStateFlags::FAILSAFE.bits(), 0);
}

#[test]
fn clear_releases_the_failsafe() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    trip_failsafe(&mut device);
    assert!(device.fpga().failsafe());

    device.send(&frame(0, Cmd::Clear, &[]));
    assert!(!device.fpga().failsafe());
}
