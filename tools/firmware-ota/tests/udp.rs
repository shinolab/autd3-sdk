use std::time::Duration;

use autd3_cpu_wire::update::RunningImage;
use autd3_rs::TransportOption;
use autd3_rs::protocol::Cmd;
use autd3_rs_firmware_emulator::autd3_cpu_fw::{
    FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH,
};
use autd3_rs_firmware_emulator::udp::UdpEmulator;
use autd3_rs_firmware_ota::{
    CpuFirmwareImage, DEFAULT_HEARTBEAT, Driver, DriverError, FpgaFirmwareImage, UdpLink,
    UpdateProgress,
};

const CONFIRMED: Option<RunningImage> = Some(RunningImage::Confirmed);

fn link(emulator: &UdpEmulator) -> UdpLink {
    let option = TransportOption {
        iface: emulator.interface(),
        open_timeout: Duration::from_secs(6),
        ack_timeout: Duration::from_millis(300),
        sync_on_open: false,
        ..TransportOption::default()
    };
    UdpLink::open(&option, emulator.num_devices(), DEFAULT_HEARTBEAT).unwrap()
}

fn open(emulator: &UdpEmulator) -> Driver {
    Driver::new(link(emulator))
}

#[test]
#[ignore = "depends on real time"]
fn the_reads_go_through_the_client() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);

    assert_eq!(driver.num_devices(), 2);
    let infos = driver.read_firmware_info().unwrap();
    assert_eq!(infos.len(), 2);
    for info in infos {
        assert_eq!(
            info.cpu_version,
            [FW_VERSION_MAJOR, FW_VERSION_MINOR, FW_VERSION_PATCH]
        );
    }
    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
    driver.close().unwrap();
}

#[test]
#[ignore = "depends on real time"]
fn a_cpu_update_streams_and_commits_the_image_on_every_device() {
    use autd3_cpu_wire::update::{ImageHeader, Slot};
    use zerocopy::FromBytes;

    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);
    let image = CpuFirmwareImage::from_slot_image(vec![0x5A; 5_000]).unwrap();
    let mut last = None;
    driver
        .update(&image, |progress| last = Some(progress))
        .unwrap();
    assert_eq!(
        last,
        Some(UpdateProgress {
            sent: image.len(),
            total: image.len(),
        })
    );
    for device in 0..emulator.num_devices() {
        emulator.with_device(device, |device| {
            let flash = device.fpga().cpu_flash();
            let header = ImageHeader::read_from_bytes(
                &flash[Slot::B.base() as usize..][..size_of::<ImageHeader>()],
            )
            .unwrap();
            assert!(header.is_plausible());
            assert!(header.is_trial());
            assert_eq!(header.crc32.get(), image.crc32());
            assert_eq!(
                &flash[Slot::B.image_base() as usize..][..image.len()],
                image.as_bytes()
            );
        });
    }
}

#[test]
#[ignore = "depends on real time"]
fn an_fpga_update_streams_the_image_to_the_update_slot_on_every_device() {
    use autd3_cpu_wire::fpga_update::{FPGA_IMAGE_BASE, FPGA_USR_ACCESS_UPDATE};
    use autd3_rs_core::fpga_bitstream::SYNC_WORD;

    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);
    let mut words = vec![
        0xFFFF_FFFF,
        0x0000_00BB,
        0x1122_0044,
        0xFFFF_FFFF,
        SYNC_WORD,
        0x2000_0000,
        0x3001_A001,
        FPGA_USR_ACCESS_UPDATE,
        0x3000_4000,
        0x5000_0400,
    ];
    words.extend([0x1234_5678; 1024]);
    words.extend([0x3000_8001, 0x0000_000D, 0x2000_0000]);
    let image = FpgaFirmwareImage::from_update_bin(
        words.iter().flat_map(|word| word.to_be_bytes()).collect(),
    )
    .unwrap();
    driver.update_fpga(&image, |_| {}).unwrap();
    for device in 0..emulator.num_devices() {
        emulator.with_device(device, |device| {
            assert_eq!(
                &device.fpga().fpga_flash()[FPGA_IMAGE_BASE as usize..][..image.len()],
                image.as_bytes()
            );
        });
    }
}

#[test]
#[ignore = "depends on real time"]
fn a_lost_frame_fails_the_request_without_sending_it_again() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);

    emulator.drop_next_frames(1, 1);

    assert!(matches!(
        driver.read_firmware_info(),
        Err(DriverError::Timeout {
            cmd: Cmd::ReadFirmwareInfo
        })
    ));
    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
}

#[test]
#[ignore = "depends on real time"]
fn a_lost_reply_fails_the_request_although_the_device_ran_it() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);

    emulator.drop_next_replies(0, 1);
    assert!(matches!(
        driver.confirm(),
        Err(DriverError::Timeout {
            cmd: Cmd::UpdateConfirm
        })
    ));

    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
}

#[test]
#[ignore = "depends on real time"]
fn a_device_that_stops_answering_fails_the_request() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let mut driver = open(&emulator);

    emulator.set_muted(1, true);
    let error = driver.read_firmware_info().unwrap_err();

    assert!(matches!(
        error,
        DriverError::Timeout {
            cmd: Cmd::ReadFirmwareInfo
        }
    ));
}

#[test]
#[ignore = "depends on real time"]
fn a_closed_link_fails_the_next_request_with_a_network_error() {
    let emulator = UdpEmulator::spawn(1).unwrap();
    let mut link = link(&emulator);
    link.close().unwrap();

    assert!(matches!(
        Driver::new(link).read_firmware_info(),
        Err(DriverError::Network(_))
    ));
}
