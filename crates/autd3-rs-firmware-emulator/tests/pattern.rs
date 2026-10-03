#![allow(clippy::cast_possible_truncation)]

use autd3_rs_core::params::REP_INFINITE;
use autd3_rs_core::protocol::{Cmd, Seq, TxFrame};
use autd3_rs_core::value::{Intensity, Phase, TransitionMode};
use autd3_rs_firmware_emulator::Device;

const NUM_TRANSDUCERS: usize = 249;
const BANK: u8 = 1;

fn frame(seq: u8, cmd: Cmd, payload: &[u8]) -> Vec<u8> {
    TxFrame::with_payload(Seq::new(seq), cmd, payload).to_vec()
}

#[test]
fn raw_pattern_round_trips_to_emissions() {
    let expected: (Vec<Phase>, Vec<Intensity>) = (
        (0..NUM_TRANSDUCERS).map(|i| Phase(i as u8)).collect(),
        (0..NUM_TRANSDUCERS)
            .map(|i| Intensity((255 - i) as u8))
            .collect(),
    );

    let mut write = vec![BANK, 1];
    write.extend_from_slice(&0u16.to_le_bytes());
    write.extend(expected.0.iter().map(|p| p.0));
    write.extend(expected.1.iter().map(|i| i.0));

    let mut config = vec![0u8; 14];
    config[0] = BANK;
    config[1] = 0x01;
    config[2..4].copy_from_slice(&512u16.to_le_bytes());
    config[4..8].copy_from_slice(&1u32.to_le_bytes());
    config[8] = 0;
    config[10..12].copy_from_slice(&0u16.to_le_bytes());
    config[12..14].copy_from_slice(&REP_INFINITE.to_le_bytes());

    let mut change = vec![0u8; 14];
    change[0] = BANK;
    change[1] = TransitionMode::Immediate.try_as_wire().unwrap().as_u8();

    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device.send(&frame(0, Cmd::WritePatternRaw, &write)).status,
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

    assert_eq!(u16::from(BANK), device.fpga().req_pattern_bank());
    assert_eq!(0x01, device.fpga().pattern_mode(BANK as usize));
    assert_eq!(expected, device.fpga().emissions_at(BANK as usize, 0));
}

fn config_change(bank: u8) -> (Vec<u8>, Vec<u8>) {
    let mut config = vec![0u8; 14];
    config[0] = bank;
    config[1] = 0x01;
    config[2..4].copy_from_slice(&512u16.to_le_bytes());
    config[4..8].copy_from_slice(&4u32.to_le_bytes());
    let mut change = vec![0u8; 14];
    change[0] = bank;
    (config, change)
}

fn send_phase_pattern(write: &[u8]) -> Device {
    let (config, change) = config_change(BANK);
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));
    assert_eq!(
        device.send(&frame(0, Cmd::WritePatternPhase, write)).status,
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
    device
}

#[test]
fn phase_bits8_pattern_expands_to_consecutive_indices() {
    let phase = |g: usize, t: usize| (t * 3 + g * 17) as u8;

    let mut write = vec![BANK, 8, 4, 0x80];
    write.extend_from_slice(&0u16.to_le_bytes());
    for g in 0..4 {
        write.extend((0..NUM_TRANSDUCERS).map(|t| phase(g, t)));
    }

    let device = send_phase_pattern(&write);
    for g in 0..4 {
        let (phases, intensities) = device.fpga().emissions_at(BANK as usize, g);
        for t in 0..NUM_TRANSDUCERS {
            assert_eq!(phases[t], Phase(phase(g, t)), "g={g} t={t}");
            assert_eq!(intensities[t], Intensity(0x80), "g={g} t={t}");
        }
    }
}

#[test]
fn phase_bits4_pattern_expands_nibbles_to_full_range() {
    let nibble = |g: usize, t: usize| ((t + g) & 0x0F) as u8;

    let mut write = vec![BANK, 4, 4, 0xFF];
    write.extend_from_slice(&0u16.to_le_bytes());
    for g in 0..4 {
        write.extend((0..NUM_TRANSDUCERS.div_ceil(2)).map(|i| {
            let hi = if 2 * i + 1 < NUM_TRANSDUCERS {
                nibble(g, 2 * i + 1)
            } else {
                0
            };
            nibble(g, 2 * i) | (hi << 4)
        }));
    }

    let device = send_phase_pattern(&write);
    for g in 0..4 {
        let (phases, intensities) = device.fpga().emissions_at(BANK as usize, g);
        for t in 0..NUM_TRANSDUCERS {
            assert_eq!(phases[t], Phase(nibble(g, t) * 0x11), "g={g} t={t}");
            assert_eq!(intensities[t], Intensity(0xFF), "g={g} t={t}");
        }
    }
}

#[test]
fn unknown_command_reports_error() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset, &[]));

    let mut bad = frame(0, Cmd::ReadErrorDetail, &[]);
    bad[1] = 0x7F;
    let rx = device.send(&bad);

    assert_eq!(rx.status, 0x01);
}
