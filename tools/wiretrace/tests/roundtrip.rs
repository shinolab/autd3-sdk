use std::path::PathBuf;
use std::time::Duration;

use autd3_cpu_wire::Cmd;
use autd3_rs_core::protocol::{RX_FRAME_BYTES, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs_firmware_emulator::Device;
use autd3_rs_link_echocat::master::init::{INPUT_BYTES, OUTPUT_BYTES};
use autd3_rs_link_echocat::sim::{EscSim, ProcessData};
use autd3_rs_link_echocat::{Master, MasterConfig};
use autd3_rs_wiretrace::tap::PcapTap;
use autd3_rs_wiretrace::{capture, cycle, replay};

const NUM_TRANSDUCERS: usize = 249;

struct FirmwareProcessData(Device);

impl ProcessData for FirmwareProcessData {
    fn exchange(&mut self, outputs: &[u8], inputs: &mut [u8]) {
        let frame: &[u8; TX_FRAME_BYTES] = outputs.try_into().expect("626 output bytes");
        let rx = self.0.send(frame);
        let mut encoded = [0u8; RX_FRAME_BYTES];
        rx.write_to(&mut encoded);
        inputs.copy_from_slice(&encoded);
    }
}

fn test_config() -> MasterConfig {
    MasterConfig {
        cycle: Duration::from_millis(1),
        dc_static_sync_iterations: 32,
        dc_start_delay: Duration::from_millis(10),
        ..MasterConfig::default()
    }
}

fn capture_path(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("autd3-wiretrace-{name}.pcap"));
    let _ = std::fs::remove_file(&path);
    path
}

fn staged(seq: u8, devices: usize) -> Vec<u8> {
    let mut tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    for (device, buf) in tx
        .as_chunks_mut::<TX_FRAME_BYTES>()
        .0
        .iter_mut()
        .enumerate()
    {
        let mut frame = TxFrame::new(Seq::new(seq), Cmd::Nop);
        frame.payload[0] = u8::try_from(device).expect("device index fits in u8");
        frame.payload[1] = seq.wrapping_mul(3);
        frame.write_to(buf);
    }
    tx
}

fn run(devices: usize, cycles: usize, path: &PathBuf, drop_at: Option<usize>) -> Vec<Vec<u8>> {
    let sim = EscSim::with_process_data(devices, Duration::from_millis(1), |_| {
        Box::new(FirmwareProcessData(Device::new(NUM_TRANSDUCERS)))
    });
    let tap = PcapTap::new(sim, path).expect("the capture file is writable");
    let mut master = Master::open(tap, test_config()).expect("the simulated bus reaches OP");

    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];
    let mut sent = Vec::new();
    for index in 0..cycles {
        if drop_at == Some(index) {
            master.bus_mut().inner_mut().drop_next_frames(1);
        }
        let tx = staged(
            u8::try_from(index).expect("cycle index fits in u8"),
            devices,
        );
        let _ = master.cycle(&tx, &mut rx);
        sent.push(tx);
    }
    drop(master);
    sent
}

#[test]
fn a_capture_of_a_simulated_bus_reconstructs_every_transmitted_frame() {
    let devices = 2;
    let path = capture_path("roundtrip");
    let sent = run(devices, 8, &path, None);

    let frames = capture::read(&path).expect("the capture is readable");
    let trace = cycle::assemble(&frames).expect("the capture holds an echocat bus");

    assert_eq!(trace.num_devices, devices);
    assert!(
        trace.cycles.len() >= sent.len(),
        "bringing the bus up exchanges process data too, so the capture holds at least our cycles"
    );

    let ours = &trace.cycles[trace.cycles.len() - sent.len()..];
    for (index, (record, tx)) in ours.iter().zip(&sent).enumerate() {
        assert!(record.tx_complete(), "cycle {index} has a gap in its tx");
        let flattened = record.tx.concat();
        assert_eq!(flattened, *tx, "cycle {index} did not round trip");
        assert!(record.responded(), "cycle {index} lost its response");
        assert!(record.rx_valid(), "cycle {index} has a bad working counter");
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_acknowledgement_of_a_cycle_arrives_in_the_cycle_after_it() {
    let devices = 3;
    let cycles = 6;
    let path = capture_path("rx");
    let sent = run(devices, cycles, &path, None);

    let frames = capture::read(&path).expect("the capture is readable");
    let trace = cycle::assemble(&frames).expect("the capture holds an echocat bus");

    let first = trace.cycles.len() - cycles;
    for (index, tx) in sent.iter().enumerate().take(cycles - 1) {
        let ack = trace
            .ack_for(first + index)
            .expect("a cycle follows every one but the last");
        for (device, rx) in ack.iter().enumerate() {
            assert_eq!(
                rx[0], tx[0],
                "device {device} did not acknowledge the sequence staged one cycle earlier"
            );
        }
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_dropped_frame_shows_up_as_a_cycle_without_a_response() {
    let devices = 2;
    let cycles = 6;
    let path = capture_path("dropped");
    run(devices, cycles, &path, Some(3));

    let frames = capture::read(&path).expect("the capture is readable");
    let trace = cycle::assemble(&frames).expect("the capture holds an echocat bus");

    let ours = &trace.cycles[trace.cycles.len() - cycles..];
    let lost = ours
        .iter()
        .enumerate()
        .filter(|(_, record)| !record.rx_valid())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(lost, vec![3], "the injected drop is the only invalid cycle");
    assert!(
        ours[3].tx_complete(),
        "the outgoing frame was captured even though its reply never came back"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn replaying_a_clean_capture_reproduces_every_acknowledgement() {
    let devices = 2;
    let path = capture_path("replay");
    run(devices, 10, &path, None);

    let frames = capture::read(&path).expect("the capture is readable");
    let trace = cycle::assemble(&frames).expect("the capture holds an echocat bus");
    let (_, report) = replay::replay(&trace, NUM_TRANSDUCERS);

    assert_eq!(report.cycles_fed, trace.cycles.len());
    assert_eq!(report.cycles_unconfirmed, 0);
    assert!(
        report.cycles_compared > 0,
        "something was actually compared"
    );
    assert!(
        report.agrees(),
        "the emulator diverged from the captured bus: {:?}",
        report.diffs
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn replaying_a_capture_with_a_dropped_frame_still_converges() {
    let devices = 2;
    let path = capture_path("replay-dropped");
    run(devices, 10, &path, Some(4));

    let frames = capture::read(&path).expect("the capture is readable");
    let trace = cycle::assemble(&frames).expect("the capture holds an echocat bus");
    let (_, report) = replay::replay(&trace, NUM_TRANSDUCERS);

    assert!(
        report.cycles_unconfirmed > 0,
        "a cycle whose delivery was never confirmed is reported, not fed to the emulator"
    );
    assert!(
        report.agrees(),
        "a dropped reply must not make the surviving cycles disagree: {:?}",
        report.diffs
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
#[ignore = "writes a sample capture for the docs instead of asserting anything"]
fn write_a_sample_capture_for_the_cli() {
    let path = std::path::PathBuf::from(
        std::env::var("WIRETRACE_SAMPLE").unwrap_or_else(|_| "sample.pcap".to_owned()),
    );
    run(2, 12, &path, Some(5));
    println!("wrote {}", path.display());
}
