use autd3_cpu_fw::params::{VERSION_NUM_MAJOR, VERSION_NUM_MINOR, VERSION_NUM_PATCH};
use autd3_cpu_fw::version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};
use autd3_cpu_wire::payload::FirmwareInfo;
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode, FRAME_BYTES_MAX, Seq, TxFrame};
use autd3_rs_firmware_emulator::{Audit, Device};
use zerocopy::FromBytes;

const ADDR_SYNC_CYCLE_0: u16 = 0x14;

const NUM_TRANSDUCERS: usize = 249;

fn frame(seq: u8, cmd: Cmd) -> [u8; FRAME_BYTES_MAX] {
    let mut buf = [0u8; FRAME_BYTES_MAX];
    TxFrame::new(Seq::new(seq), cmd).write_to(&mut buf);
    buf
}

#[test]
fn reset_acks_with_sentinel() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    let rx = device.send(&frame(0, Cmd::Reset));
    assert_eq!(rx.ack, 0xFF);
    assert_eq!(rx.status, 0);
}

#[test]
fn reads_the_firmware_info_in_one_frame() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    let (major, minor, patch) = device.fpga().fpga_version();
    device.send(&frame(0, Cmd::Reset));

    let rx = device.send(&frame(0, Cmd::ReadFirmwareInfo));
    assert_eq!(rx.ack, 0);
    assert_eq!(rx.status, 0);
    let info = FirmwareInfo::read_from_bytes(rx.data()).unwrap();
    assert_eq!(
        info.cpu_version,
        [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH]
    );
    assert_eq!(
        info.fpga_version.map(u16::from),
        [major & 0xFF, minor, patch]
    );
}

#[test]
fn fpga_reports_version_after_init() {
    let device = Device::new(NUM_TRANSDUCERS);
    let expected = (
        u16::from(VERSION_NUM_MAJOR),
        u16::from(VERSION_NUM_MINOR),
        u16::from(VERSION_NUM_PATCH),
    );
    assert_eq!(device.fpga().fpga_version(), expected);
}

#[test]
fn init_enables_all_outputs_by_default() {
    let device = Device::new(NUM_TRANSDUCERS);
    assert!((0..NUM_TRANSDUCERS).all(|i| device.fpga().output_mask_enabled(i)));
}

#[test]
fn synchronize_writes_the_fixed_sync_cycle() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    device.send(&frame(0, Cmd::Reset));

    let rx = device.send(&frame(0, Cmd::Synchronize));
    assert_eq!(rx.ack, 0);
    assert_eq!(rx.status, DeviceErrorCode::None as u8);
    assert_eq!(device.fpga().controller_reg(ADDR_SYNC_CYCLE_0), 20480);
}

#[test]
fn audit_drives_multiple_independent_devices() {
    let mut audit = Audit::new([NUM_TRANSDUCERS, NUM_TRANSDUCERS, NUM_TRANSDUCERS]);
    assert_eq!(audit.num_devices(), 3);

    let tx = frame(0, Cmd::Reset);
    let replies = audit.send(&[&tx, &tx, &tx], 7);
    assert_eq!(replies.len(), 3);
    assert!(
        replies
            .iter()
            .enumerate()
            .all(|(i, r)| r.device == i && r.msg_id == 7 && r.reply.ack == 0xFF)
    );
}
