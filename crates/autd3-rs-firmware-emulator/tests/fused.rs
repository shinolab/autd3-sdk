#![allow(clippy::cast_possible_truncation)]

use autd3_rs_core::protocol::{Cmd, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs_core::value::{Emission, Intensity, Phase};
use autd3_rs_firmware_emulator::Device;

const NUM_TRANSDUCERS: usize = 249;
const BANK: u8 = 1;
const DIVIDER: u16 = 512;

fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> [u8; TX_FRAME_BYTES] {
    let mut tx = TxFrame::new(Seq::new(seq), cmd);
    tx.payload[..payload.len()].copy_from_slice(payload);
    let mut buf = [0u8; TX_FRAME_BYTES];
    tx.write_to(&mut buf);
    buf
}

fn expected_emissions() -> Vec<Emission> {
    (0..NUM_TRANSDUCERS)
        .map(|i| Emission {
            phase: Phase(i as u8),
            intensity: Intensity((255 - i) as u8),
        })
        .collect()
}

fn emission_bytes(emissions: &[Emission]) -> Vec<u8> {
    emissions
        .iter()
        .flat_map(|e| [e.phase.0, e.intensity.0])
        .collect()
}

fn split_path(emissions: &[Emission]) -> Device {
    let mut write = vec![BANK, 0];
    write.extend_from_slice(&0u32.to_le_bytes());
    write.extend_from_slice(&((NUM_TRANSDUCERS * 2) as u16).to_le_bytes());
    write.extend_from_slice(&emission_bytes(emissions));

    let mut config = vec![0u8; 14];
    config[0] = BANK;
    config[1] = 0x01;
    config[2..4].copy_from_slice(&DIVIDER.to_le_bytes());
    config[4..8].copy_from_slice(&1u32.to_le_bytes());
    config[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes());

    let mut change = vec![0u8; 14];
    change[0] = BANK;
    change[1] = 0xFF;

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
    device
}

fn fused_path(emissions: &[Emission]) -> Device {
    let mut p = vec![0u8; 32];
    p[0] = BANK;
    p[1] = 0x01;
    p[2..4].copy_from_slice(&DIVIDER.to_le_bytes());
    p[4..8].copy_from_slice(&1u32.to_le_bytes());
    p[8] = 0;
    p[9] = 0xFF;
    p[10..12].copy_from_slice(&0u16.to_le_bytes());
    p[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes());
    p[14..16].copy_from_slice(&((NUM_TRANSDUCERS * 2) as u16).to_le_bytes());
    p.extend_from_slice(&emission_bytes(emissions));

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device.send(&frame(0, Cmd::WritePatternFused, &p)).data,
        0,
        "fused frame must be accepted"
    );
    device
}

#[test]
fn fused_pattern_produces_the_same_drives_as_the_three_frame_path() {
    let emissions = expected_emissions();

    let split = split_path(&emissions);
    let fused = fused_path(&emissions);

    assert_eq!(emissions, fused.fpga().drives_at(BANK as usize, 0));
    assert_eq!(
        split.fpga().drives_at(BANK as usize, 0),
        fused.fpga().drives_at(BANK as usize, 0),
        "fused path must drive transducers identically"
    );
    assert_eq!(
        split.fpga().req_pattern_bank(),
        fused.fpga().req_pattern_bank()
    );
    assert_eq!(
        split.fpga().pattern_mode(BANK as usize),
        fused.fpga().pattern_mode(BANK as usize)
    );
}

#[test]
fn fused_modulation_produces_the_same_state_as_the_three_frame_path() {
    let data: Vec<u8> = (0..64u16).map(|i| (i ^ 0x5A) as u8).collect();

    let mut write = vec![BANK, 0];
    write.extend_from_slice(&0u32.to_le_bytes());
    write.extend_from_slice(&(data.len() as u16).to_le_bytes());
    write.extend_from_slice(&data);

    let mut config = vec![0u8; 10];
    config[0] = BANK;
    config[2..4].copy_from_slice(&DIVIDER.to_le_bytes());
    config[4..8].copy_from_slice(&(data.len() as u32).to_le_bytes());
    config[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes());

    let mut change = vec![0u8; 14];
    change[0] = BANK;
    change[1] = 0xFF;

    let mut split = Device::new(NUM_TRANSDUCERS);
    split.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        split
            .send(&frame(0, Cmd::WriteModulationBuffer, &write))
            .data,
        0
    );
    assert_eq!(
        split.send(&frame(1, Cmd::ConfigModulation, &config)).data,
        0
    );
    assert_eq!(
        split
            .send(&frame(2, Cmd::ChangeModulationBank, &change))
            .data,
        0
    );

    let mut p = vec![0u8; 24];
    p[0] = BANK;
    p[1] = 0xFF;
    p[2..4].copy_from_slice(&DIVIDER.to_le_bytes());
    p[4..8].copy_from_slice(&(data.len() as u32).to_le_bytes());
    p[8..10].copy_from_slice(&0xFFFFu16.to_le_bytes());
    p[10..12].copy_from_slice(&(data.len() as u16).to_le_bytes());
    p.extend_from_slice(&data);

    let mut fused = Device::new(NUM_TRANSDUCERS);
    fused.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        fused.send(&frame(0, Cmd::WriteModulationFused, &p)).data,
        0,
        "fused modulation frame must be accepted"
    );

    assert_eq!(
        split.fpga().req_modulation_bank(),
        fused.fpga().req_modulation_bank(),
        "fused modulation must arm the same bank"
    );
    assert_eq!(
        split.fpga().modulation_buffer(BANK as usize),
        fused.fpga().modulation_buffer(BANK as usize),
        "fused modulation must land identical samples"
    );
    assert_eq!(
        split.fpga().modulation_cycle(BANK as usize),
        fused.fpga().modulation_cycle(BANK as usize)
    );
    assert_eq!(
        split.fpga().modulation_freq_div(BANK as usize),
        fused.fpga().modulation_freq_div(BANK as usize)
    );
}

const ULTRASOUND_PERIOD_NS: u64 = 25_000;

#[test]
fn fused_modulation_finite_loop_arms_and_stops_from_a_single_latch() {
    let samples: [u8; 4] = [10, 20, 30, 40];
    let bank = 1u8;
    let rep = 1u16;

    let mut p = vec![0u8; 24];
    p[0] = bank;
    p[1] = 0x00;
    p[2..4].copy_from_slice(&1u16.to_le_bytes());
    p[4..8].copy_from_slice(&(samples.len() as u32).to_le_bytes());
    p[8..10].copy_from_slice(&rep.to_le_bytes());
    p[10..12].copy_from_slice(&(samples.len() as u16).to_le_bytes());
    p.extend_from_slice(&samples);

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device.send(&frame(0, Cmd::WriteModulationFused, &p)).data,
        0,
        "a finite loop with a SYNC_IDX transition must be accepted"
    );

    let mut indices = Vec::new();
    for i in 0..24u64 {
        device
            .fpga_mut()
            .update_with_sys_time(i * ULTRASOUND_PERIOD_NS);
        indices.push(device.fpga().current_mod_idx());
    }

    assert_eq!(
        device.fpga().current_mod_bank(),
        bank as usize,
        "the single latch must have armed and fired the bank transition"
    );
    assert_eq!(*indices.last().unwrap(), samples.len() - 1, "{indices:?}");
    assert!(
        indices.windows(2).rev().take(4).all(|w| w[0] == w[1]),
        "finite loop must stop after rep: {indices:?}"
    );
}
