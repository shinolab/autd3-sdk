use autd3_rs_core::link::{CycleOutcome, Link};
use autd3_rs_core::protocol::{Cmd, RX_FRAME_BYTES, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs_firmware_emulator::{Audit, Device};

const NUM_TRANSDUCERS: usize = 249;

fn frame(seq: u8, cmd: Cmd) -> [u8; TX_FRAME_BYTES] {
    let mut buf = [0u8; TX_FRAME_BYTES];
    TxFrame::new(Seq::new(seq), cmd).write_to(&mut buf);
    buf
}

#[test]
fn reset_acks_with_sentinel() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    let rx = device.send(&frame(0, Cmd::Reset));
    assert_eq!(rx.ack, Seq::new(0xFF));
    assert_eq!(rx.data, 0);
}

#[test]
fn reads_cpu_firmware_version() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset));

    let major = device.send(&frame(0, Cmd::ReadCpuFwVersionMajor));
    assert_eq!(major.ack, Seq::new(0));
    assert_eq!(major.data, 0);

    let minor = device.send(&frame(1, Cmd::ReadCpuFwVersionMinor));
    assert_eq!(minor.ack, Seq::new(1));
    assert_eq!(minor.data, 1);

    let patch = device.send(&frame(2, Cmd::ReadCpuFwVersionPatch));
    assert_eq!(patch.ack, Seq::new(2));
    assert_eq!(patch.data, 0);
}

#[test]
fn fpga_reports_version_after_init() {
    let device = Device::new(NUM_TRANSDUCERS);
    assert_eq!(device.fpga().fpga_version(), (0, 1, 0));
}

#[test]
fn init_enables_all_outputs_by_default() {
    let device = Device::new(NUM_TRANSDUCERS);
    assert!((0..NUM_TRANSDUCERS).all(|i| device.fpga().output_mask_enabled(i)));
}

#[test]
fn link_drives_multiple_independent_devices() {
    let mut link = Audit::new([NUM_TRANSDUCERS, NUM_TRANSDUCERS, NUM_TRANSDUCERS]);
    assert_eq!(link.num_devices(), 3);

    let tx = vec![frame(0, Cmd::Reset); 3];
    let mut rx = vec![[0u8; RX_FRAME_BYTES]; 3];
    let CycleOutcome { rx_valid } = link.cycle(&tx, &mut rx).unwrap();
    assert!(rx_valid);
    assert!(rx.iter().all(|r| r == &[0xFF, 0x00]));
}
