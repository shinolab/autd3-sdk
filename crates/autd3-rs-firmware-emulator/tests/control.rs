use autd3_rs_core::protocol::{Cmd, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs_firmware_emulator::Device;

const NUM_TRANSDUCERS: usize = 249;

fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> [u8; TX_FRAME_BYTES] {
    let mut buf = [0u8; TX_FRAME_BYTES];
    TxFrame::new(Seq::new(seq), cmd).write_to(&mut buf);
    buf[2..2 + payload.len()].copy_from_slice(payload);
    buf
}

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
        payload[8 * i..8 * i + 8].copy_from_slice(&v.to_le_bytes());
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
        payload[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
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
fn read_fpga_state_returns_register() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    device.fpga_mut().set_fpga_state(0x83);
    let rx = device.send(&frame(0, Cmd::ReadFpgaState, &[]));
    assert_eq!(rx.data, 0x83);
}
