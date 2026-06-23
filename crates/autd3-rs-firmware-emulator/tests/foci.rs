#![allow(clippy::cast_possible_truncation)]

use autd3_rs_core::protocol::{Cmd, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs_core::value::Intensity;
use autd3_rs_firmware_emulator::Device;

const NUM_TRANSDUCERS: usize = 249;
const BANK: u8 = 0;
const FOCUS_INTENSITY: u8 = 0xAA;

fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> [u8; TX_FRAME_BYTES] {
    let mut tx = TxFrame::new(Seq::new(seq), cmd);
    tx.payload[..payload.len()].copy_from_slice(payload);
    let mut buf = [0u8; TX_FRAME_BYTES];
    tx.write_to(&mut buf);
    buf
}

#[test]
fn single_focus_synthesizes_phases() {
    let z: u64 = 8192;
    let focus: u64 = (z << 36) | (u64::from(FOCUS_INTENSITY) << 54);

    let mut write = vec![BANK, 0];
    write.extend_from_slice(&0u32.to_le_bytes());
    write.extend_from_slice(&8u16.to_le_bytes());
    write.extend_from_slice(&focus.to_le_bytes());

    let mut config = vec![0u8; 12];
    config[0] = BANK;
    config[1] = 0x00;
    config[2..4].copy_from_slice(&512u16.to_le_bytes());
    config[4..8].copy_from_slice(&1u32.to_le_bytes());
    config[8] = 1;
    config[10..12].copy_from_slice(&340u16.to_le_bytes());

    let change = {
        let mut c = vec![BANK, 0x00];
        c.extend_from_slice(&0u64.to_le_bytes());
        c
    };

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
    device.fpga_mut().update_with_sys_time(0);

    assert_eq!(0x00, device.fpga().pattern_mode(BANK as usize));
    assert_eq!(1, device.fpga().num_foci(BANK as usize));

    let drives = device.fpga().drives();
    assert_eq!(NUM_TRANSDUCERS, drives.len());

    assert!(
        drives
            .iter()
            .all(|d| d.intensity == Intensity(FOCUS_INTENSITY))
    );

    assert!(drives.iter().any(|d| d.phase != drives[0].phase));
}
