#![allow(clippy::cast_possible_truncation)]

use autd3_rs_core::protocol::{Cmd, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs_firmware_emulator::Device;

const NUM_TRANSDUCERS: usize = 249;
const ULTRASOUND_PERIOD_NS: u64 = 25_000;

fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> [u8; TX_FRAME_BYTES] {
    let mut tx = TxFrame::new(Seq::new(seq), cmd);
    tx.payload[..payload.len()].copy_from_slice(payload);
    let mut buf = [0u8; TX_FRAME_BYTES];
    tx.write_to(&mut buf);
    buf
}

#[test]
fn modulation_buffer_and_index_follow_time() {
    let samples: [u8; 4] = [10, 20, 30, 40];
    let bank = 0u8;
    let divider = 1u16;

    let mut write = vec![bank, 0];
    write.extend_from_slice(&0u32.to_le_bytes());
    write.extend_from_slice(&(samples.len() as u16).to_le_bytes());
    write.extend_from_slice(&samples);

    let mut config = vec![bank, 0];
    config.extend_from_slice(&divider.to_le_bytes());
    config.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    config.extend_from_slice(&0xFFFFu16.to_le_bytes());

    let mut change = vec![bank, 0x00];
    change.extend_from_slice(&0u64.to_le_bytes());

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device
            .send(&frame(0, Cmd::WriteModulationBuffer, &write))
            .data,
        0
    );
    assert_eq!(
        device.send(&frame(1, Cmd::ConfigModulation, &config)).data,
        0
    );
    assert_eq!(
        device
            .send(&frame(2, Cmd::ChangeModulationBank, &change))
            .data,
        0
    );

    assert_eq!(samples.len(), device.fpga().modulation_cycle(bank as usize));
    assert_eq!(
        samples.to_vec(),
        device.fpga().modulation_buffer(bank as usize)
    );

    for (i, &expected) in [10u8, 20, 30, 40, 10, 20].iter().enumerate() {
        device
            .fpga_mut()
            .update_with_sys_time(i as u64 * ULTRASOUND_PERIOD_NS);
        assert_eq!(i % 4, device.fpga().current_mod_idx());
        assert_eq!(expected, device.fpga().modulation());
    }
}

#[test]
fn modulation_finite_loop_stops_after_rep() {
    let samples: [u8; 4] = [10, 20, 30, 40];
    let bank = 1u8;
    let divider = 1u16;
    let rep = 1u16; 

    let mut write = vec![bank, 0];
    write.extend_from_slice(&0u32.to_le_bytes());
    write.extend_from_slice(&(samples.len() as u16).to_le_bytes());
    write.extend_from_slice(&samples);

    let mut config = vec![bank, 0];
    config.extend_from_slice(&divider.to_le_bytes());
    config.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    config.extend_from_slice(&rep.to_le_bytes());

    let mut change = vec![bank, 0x00];
    change.extend_from_slice(&0u64.to_le_bytes());

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    device.send(&frame(0, Cmd::WriteModulationBuffer, &write));
    device.send(&frame(1, Cmd::ConfigModulation, &config));
    device.send(&frame(2, Cmd::ChangeModulationBank, &change));

    let mut indices = Vec::new();
    for i in 0..24u64 {
        device
            .fpga_mut()
            .update_with_sys_time(i * ULTRASOUND_PERIOD_NS);
        indices.push(device.fpga().current_mod_idx());
    }

    assert_eq!(*indices.last().unwrap(), samples.len() - 1, "{indices:?}");
    assert!(
        indices.windows(2).rev().take(4).all(|w| w[0] == w[1]),
        "playback must be stopped (index frozen): {indices:?}"
    );
}
