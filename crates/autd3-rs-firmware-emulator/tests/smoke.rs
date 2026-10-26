mod common;

use autd3_cpu_fw::fpga_params::{VERSION_NUM_MAJOR, VERSION_NUM_MINOR, VERSION_NUM_PATCH};
use autd3_cpu_fw::version::{FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH};
use autd3_cpu_wire::payload::FirmwareInfo;
use autd3_rs_core::protocol::{Cmd, DeviceErrorCode};
use autd3_rs_firmware_emulator::Device;
use zerocopy::FromBytes;

use common::{NUM_TRANSDUCERS, frame};

#[test]
fn reads_the_firmware_info_in_one_frame() {
    let mut device = Device::new(NUM_TRANSDUCERS);
    let (major, minor, patch) = device.fpga().fpga_version();

    let rx = device.send(&frame(Cmd::ReadFirmwareInfo, &[]));
    assert_eq!(rx.status, DeviceErrorCode::None);
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
