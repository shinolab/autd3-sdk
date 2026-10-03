#![allow(clippy::cast_possible_truncation)]

use autd3_rs_core::params::REP_INFINITE;
use autd3_rs_core::protocol::{Cmd, Seq, TxFrame};
use autd3_rs_core::value::{Intensity, TransitionMode};
use autd3_rs_firmware_emulator::Device;

const NUM_TRANSDUCERS: usize = 249;
const BANK: u8 = 0;
const FOCUS_INTENSITY: u8 = 0xAA;

fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> Vec<u8> {
    TxFrame::with_payload(Seq::new(seq), cmd, payload).to_vec()
}

#[test]
fn single_focus_synthesizes_phases() {
    let z: u64 = 8192;
    let focus: u64 = (z << 36) | (u64::from(FOCUS_INTENSITY) << 54);

    let mut write = vec![BANK, 0];
    write.extend_from_slice(&0u32.to_le_bytes());
    write.extend_from_slice(&focus.to_le_bytes());
    write.extend_from_slice(&focus.to_le_bytes());

    let mut config = vec![0u8; 14];
    config[0] = BANK;
    config[1] = 0x00;
    config[2..4].copy_from_slice(&512u16.to_le_bytes());
    config[4..8].copy_from_slice(&2u32.to_le_bytes());
    config[8] = 1;
    config[10..12].copy_from_slice(&340u16.to_le_bytes());
    config[12..14].copy_from_slice(&REP_INFINITE.to_le_bytes());

    let change = {
        let mut c = vec![
            BANK,
            TransitionMode::Immediate.try_as_wire().unwrap().as_u8(),
        ];
        c.extend_from_slice(&0u64.to_le_bytes());
        c.extend_from_slice(&0u32.to_le_bytes());
        c
    };

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device.send(&frame(0, Cmd::WriteFociBuffer, &write)).status,
        0
    );
    assert_eq!(
        device.send(&frame(1, Cmd::ConfigPattern, &config)).status,
        0
    );
    assert_eq!(
        device
            .send(&frame(2, Cmd::ChangePatternBank, &change))
            .status,
        0
    );
    device.fpga_mut().update_with_sys_time(0);

    assert_eq!(0x00, device.fpga().pattern_mode(BANK as usize));
    assert_eq!(1, device.fpga().num_foci(BANK as usize));

    let (phases, intensities) = device.fpga().emissions();
    assert_eq!(NUM_TRANSDUCERS, phases.len());
    assert_eq!(NUM_TRANSDUCERS, intensities.len());

    assert!(intensities.iter().all(|&i| i == Intensity(FOCUS_INTENSITY)));

    assert!(phases.iter().any(|&p| p != phases[0]));
}
