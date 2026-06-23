#![allow(clippy::cast_possible_truncation)]

use autd3_rs_core::protocol::{Cmd, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs_core::value::{Emission, Intensity, Phase};
use autd3_rs_firmware_emulator::Device;

const NUM_TRANSDUCERS: usize = 249;
const BANK: u8 = 1;

fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> [u8; TX_FRAME_BYTES] {
    let mut tx = TxFrame::new(Seq::new(seq), cmd);
    tx.payload[..payload.len()].copy_from_slice(payload);
    let mut buf = [0u8; TX_FRAME_BYTES];
    tx.write_to(&mut buf);
    buf
}

#[test]
fn raw_pattern_round_trips_to_drives() {
    let expected: Vec<Emission> = (0..NUM_TRANSDUCERS)
        .map(|i| Emission {
            phase: Phase(i as u8),
            intensity: Intensity((255 - i) as u8),
        })
        .collect();

    let mut write = Vec::new();
    write.push(BANK);
    write.push(0);
    write.extend_from_slice(&0u32.to_le_bytes());
    write.extend_from_slice(&((NUM_TRANSDUCERS * 2) as u16).to_le_bytes());
    for e in &expected {
        write.push(e.phase.0);
        write.push(e.intensity.0);
    }

    let mut config = vec![0u8; 12];
    config[0] = BANK;
    config[1] = 0x01;
    config[2..4].copy_from_slice(&512u16.to_le_bytes());
    config[4..8].copy_from_slice(&1u32.to_le_bytes());
    config[8] = 0;
    config[10..12].copy_from_slice(&0u16.to_le_bytes());

    let mut change = vec![0u8; 10];
    change[0] = BANK;
    change[1] = 0x00;

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device.send(&frame(0, Cmd::WritePatternBuffer, &write)).data,
        0
    );
    assert_eq!(device.send(&frame(1, Cmd::ConfigPattern, &config)).data, 0);
    assert_eq!(
        device.send(&frame(2, Cmd::ChangePatternBank, &change)).data,
        0
    );

    assert_eq!(u16::from(BANK), device.fpga().req_pattern_bank());
    assert_eq!(0x01, device.fpga().pattern_mode(BANK as usize));
    assert_eq!(expected, device.fpga().drives_at(BANK as usize, 0));
}

#[test]
fn unknown_command_reports_error() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    let mut bad = frame(0, Cmd::ReadErrorDetail, &[]);
    bad[1] = 0x7F;
    let rx = device.send(&bad);

    assert_eq!(rx.data, 0x01);
}
