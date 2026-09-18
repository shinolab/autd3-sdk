use autd3_cpu_wire::update::{ImageHeader, Slot, crc32};
use autd3_rs_firmware_emulator::{Audit, EMULATED_CPU_IMAGE};
use autd3_rs_firmware_ota::{CpuFirmwareImage, Driver, DriverError, UpdateProgress};
use zerocopy::FromBytes;

const NUM_TRANSDUCERS: usize = 249;

fn image(len: usize, seed: u8) -> CpuFirmwareImage {
    let body = (0..len)
        .map(|i| i.to_le_bytes()[0].wrapping_mul(31).wrapping_add(seed))
        .collect();
    CpuFirmwareImage::from_slot_image(body).unwrap()
}

fn header(audit: &Audit, device: usize, slot: Slot) -> ImageHeader {
    let flash = audit.device(device).fpga().cpu_flash();
    let base = slot.base() as usize;
    ImageHeader::read_from_bytes(&flash[base..base + core::mem::size_of::<ImageHeader>()]).unwrap()
}

fn body(audit: &Audit, device: usize, slot: Slot, len: usize) -> Vec<u8> {
    let base = slot.image_base() as usize;
    audit.device(device).fpga().cpu_flash()[base..base + len].to_vec()
}

#[test]
fn a_fresh_emulator_runs_a_generation_0_image_from_slot_a() {
    let audit = Audit::new([NUM_TRANSDUCERS]);
    let a = header(&audit, 0, Slot::A);
    assert!(a.is_plausible());
    assert_eq!(a.generation.get(), 0);
    assert_eq!(
        body(&audit, 0, Slot::A, EMULATED_CPU_IMAGE.len()),
        EMULATED_CPU_IMAGE
    );
    assert!(!header(&audit, 0, Slot::B).is_plausible());
}

fn reboot_all(audit: &mut Audit, devices: usize) {
    for device in 0..devices {
        let before = audit.device(device).fpga().reset_count();
        for _ in 0..100 {
            audit.device_mut(device).tick_1ms();
        }
        assert_eq!(audit.device(device).fpga().reset_count(), before + 1);
    }
}

#[test]
fn update_activate_confirm_then_the_next_update_takes_the_other_slot() {
    let audit = Audit::new([NUM_TRANSDUCERS, NUM_TRANSDUCERS]);
    let mut driver = Driver::open(audit).unwrap();
    assert_eq!(driver.num_devices(), 2);
    assert_eq!(driver.read_cpu_version().unwrap().len(), 2);

    let first = image(70_000, 1);
    let mut last = None;
    driver.update(&first, |p| last = Some(p)).unwrap();
    assert_eq!(
        last,
        Some(UpdateProgress {
            sent: first.len(),
            total: first.len()
        })
    );
    driver.activate().unwrap();

    let mut audit = driver.into_link();
    for device in 0..2 {
        let b = header(&audit, device, Slot::B);
        assert_eq!(b.generation.get(), 1);
        assert_eq!(b.length.get() as usize, first.len());
        assert_eq!(b.crc32.get(), crc32(first.as_bytes()));
        assert!(b.is_trial());
        assert_eq!(body(&audit, device, Slot::B, first.len()), first.as_bytes());
        assert_eq!(audit.device(device).booted_slot(), Some(Slot::A));
    }
    reboot_all(&mut audit, 2);
    for device in 0..2 {
        assert_eq!(audit.device(device).booted_slot(), Some(Slot::B));
        assert_eq!(header(&audit, device, Slot::B).attempts_used(), 1);
    }

    let mut driver = Driver::open(audit).unwrap();
    driver.confirm().unwrap();
    driver.confirm().unwrap();
    let second = image(1234, 2);
    driver.update(&second, |_| {}).unwrap();

    let mut audit = driver.into_link();
    for device in 0..2 {
        let b = header(&audit, device, Slot::B);
        assert!(!b.needs_confirmation());
        assert!(b.is_boot_eligible());
        let a = header(&audit, device, Slot::A);
        assert_eq!(a.generation.get(), 2);
        assert!(a.is_trial());
        assert_eq!(
            body(&audit, device, Slot::A, second.len()),
            second.as_bytes()
        );
        audit.device_mut(device).power_cycle();
        assert_eq!(audit.device(device).booted_slot(), Some(Slot::A));
    }
}

#[test]
fn an_unconfirmed_update_rolls_back_after_the_next_reset() {
    let audit = Audit::new([NUM_TRANSDUCERS]);
    let mut driver = Driver::open(audit).unwrap();
    let next = image(5000, 7);
    driver.update(&next, |_| {}).unwrap();
    driver.activate().unwrap();

    let mut audit = driver.into_link();
    reboot_all(&mut audit, 1);
    assert_eq!(audit.device(0).booted_slot(), Some(Slot::B));

    let mut driver = Driver::open(audit).unwrap();
    assert_eq!(driver.read_cpu_version().unwrap().len(), 1);
    let mut audit = driver.into_link();

    audit.device_mut(0).power_cycle();
    assert_eq!(audit.device(0).booted_slot(), Some(Slot::A));
    assert_eq!(
        body(&audit, 0, Slot::A, EMULATED_CPU_IMAGE.len()),
        EMULATED_CPU_IMAGE
    );

    let mut driver = Driver::open(audit).unwrap();
    driver.confirm().unwrap();
    let mut audit = driver.into_link();
    audit.device_mut(0).power_cycle();
    assert_eq!(audit.device(0).booted_slot(), Some(Slot::A));
    assert!(header(&audit, 0, Slot::B).is_trial());
}

#[test]
fn confirm_before_any_update_is_a_no_op_on_a_normal_image() {
    let audit = Audit::new([NUM_TRANSDUCERS]);
    let before = audit.device(0).fpga().cpu_flash().to_vec();
    let mut driver = Driver::open(audit).unwrap();
    driver.confirm().unwrap();
    let audit = driver.into_link();
    assert_eq!(audit.device(0).fpga().cpu_flash(), &before[..]);
}

#[test]
fn update_refuses_when_the_device_has_no_valid_slot() {
    let mut audit = Audit::new([NUM_TRANSDUCERS]);
    audit.device_mut(0).fpga_mut().cpu_flash_mut().fill(0xFF);
    let mut driver = Driver::open(audit).unwrap();
    assert!(matches!(
        driver.update(&image(100, 3), |_| {}),
        Err(DriverError::Device {
            device: 0,
            code: 0x0C,
            ..
        })
    ));
    let audit = driver.into_link();
    assert!(
        audit
            .device(0)
            .fpga()
            .cpu_flash()
            .iter()
            .all(|&b| b == 0xFF)
    );
}

#[test]
fn activation_without_a_committed_image_is_a_device_error() {
    let audit = Audit::new([NUM_TRANSDUCERS]);
    let mut driver = Driver::open(audit).unwrap();
    assert!(matches!(
        driver.activate(),
        Err(DriverError::Device {
            device: 0,
            code: 0x0D,
            ..
        })
    ));
    driver.close().unwrap();
}

mod fpga {
    use autd3_cpu_wire::fpga_update::{
        FPGA_IMAGE_BASE, FPGA_USR_ACCESS_UPDATE, FpgaBootImage, SYNC_WORD,
    };
    use autd3_rs_core::protocol::{Cmd, Seq, TxFrame};
    use autd3_rs_firmware_emulator::Audit;
    use autd3_rs_firmware_emulator::autd3_cpu_fw::Port;
    use autd3_rs_firmware_emulator::autd3_cpu_fw::params::ADDR_VERSION_NUM_MAJOR;
    use autd3_rs_firmware_ota::{DEFAULT_TIMEOUT, Driver, DriverError, FpgaFirmwareImage};

    use super::NUM_TRANSDUCERS;

    const RECONFIG_TICKS: usize = 3200;

    fn bitstream(payload: u32, seed: u32) -> FpgaFirmwareImage {
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
            0x5000_0000 | payload,
        ];
        words.extend((0..payload).map(|i| i.wrapping_mul(0x9E37_79B9) ^ seed));
        words.extend([0x3000_8001, 0x0000_000D, 0x2000_0000]);
        let bytes = words.iter().flat_map(|w| w.to_be_bytes()).collect();
        FpgaFirmwareImage::from_update_bin(bytes).unwrap()
    }

    fn slot(audit: &Audit, device: usize, len: usize) -> Vec<u8> {
        let base = FPGA_IMAGE_BASE as usize;
        audit.device(device).fpga().fpga_flash()[base..base + len].to_vec()
    }

    fn settle(audit: &mut Audit, devices: usize) {
        for device in 0..devices {
            for _ in 0..RECONFIG_TICKS {
                audit.device_mut(device).tick_1ms();
            }
        }
    }

    #[test]
    fn an_fpga_update_reconfigures_into_the_new_image() {
        let audit = Audit::new([NUM_TRANSDUCERS, NUM_TRANSDUCERS]);
        let mut driver = Driver::open(audit).unwrap();
        assert_eq!(
            driver.read_fpga_boot_image().unwrap(),
            [FpgaBootImage::Update, FpgaBootImage::Update]
        );
        let image = bitstream(40_000, 1);
        let mut last = None;
        driver.update_fpga(&image, |p| last = Some(p)).unwrap();
        assert_eq!(last.map(|p| p.sent), Some(image.len()));
        driver.activate_fpga().unwrap();

        let mut audit = driver.into_link();
        for device in 0..2 {
            assert_eq!(slot(&audit, device, image.len()), image.as_bytes());
            assert!(
                audit.device(device).fpga().fpga_flash()[..FPGA_IMAGE_BASE as usize]
                    .iter()
                    .all(|&b| b == 0xFF)
            );
            assert!(!audit.device(device).fpga().output_mask_enabled(0));
        }
        settle(&mut audit, 2);
        for device in 0..2 {
            assert_eq!(audit.device(device).fpga().reconfig_count(), 1);
            assert_eq!(audit.device(device).fpga().reset_count(), 0);
            assert!(audit.device(device).fpga().output_mask_enabled(0));
        }

        let mut driver = Driver::open(audit).unwrap();
        assert_eq!(
            driver.read_fpga_boot_image().unwrap(),
            [FpgaBootImage::Update, FpgaBootImage::Update]
        );
        assert_eq!(driver.read_fpga_version().unwrap().len(), 2);
        driver.close().unwrap();
    }

    #[test]
    fn an_fpga_that_ignores_reboot_is_reported() {
        let mut audit = Audit::new([NUM_TRANSDUCERS, NUM_TRANSDUCERS]);
        audit.device_mut(1).fpga_mut().ignore_next_reboots(u32::MAX);
        let mut driver = Driver::open(audit).unwrap();
        let image = bitstream(1000, 4);
        driver.update_fpga(&image, |_| {}).unwrap();
        driver.activate_fpga().unwrap();
        let mut audit = driver.into_link();
        for _ in 0..3 {
            settle(&mut audit, 2);
        }
        assert_eq!(audit.device(0).fpga().reconfig_count(), 1);
        assert_eq!(audit.device(1).fpga().reconfig_count(), 0);
        assert!(audit.device(1).fpga().output_mask_enabled(0));

        let mut driver = Driver::open(audit).unwrap();
        assert_eq!(
            driver.read_fpga_boot_image().unwrap(),
            [FpgaBootImage::Update, FpgaBootImage::Update]
        );
        assert!(matches!(
            driver.ensure_fpga_reconfigured(),
            Err(DriverError::FpgaReconfigFailed { device: 1 })
        ));
        driver.close().unwrap();
    }

    #[test]
    fn a_reboot_ignored_once_is_retried() {
        let mut audit = Audit::new([NUM_TRANSDUCERS]);
        audit.device_mut(0).fpga_mut().ignore_next_reboots(1);
        let mut driver = Driver::open(audit).unwrap();
        driver.update_fpga(&bitstream(1000, 5), |_| {}).unwrap();
        driver.activate_fpga().unwrap();
        let mut audit = driver.into_link();
        settle(&mut audit, 1);
        assert_eq!(audit.device(0).fpga().reconfig_count(), 1);
        assert!(!audit.device(0).fpga().output_mask_enabled(0));
        settle(&mut audit, 1);
        assert_eq!(audit.device(0).fpga().reconfig_count(), 1);
        assert!(audit.device(0).fpga().output_mask_enabled(0));
        let mut driver = Driver::open(audit).unwrap();
        driver.ensure_fpga_reconfigured().unwrap();
        driver.close().unwrap();
    }

    #[test]
    fn a_broken_slot_falls_back_to_golden() {
        let audit = Audit::new([NUM_TRANSDUCERS]);
        let mut driver = Driver::open(audit).unwrap();
        let image = bitstream(1000, 2);
        driver.update_fpga(&image, |_| {}).unwrap();
        driver.activate_fpga().unwrap();
        let mut audit = driver.into_link();
        let base = FPGA_IMAGE_BASE as usize;
        audit.device_mut(0).fpga_mut().fpga_flash_mut()[base..base + 32].fill(0xFF);
        settle(&mut audit, 1);
        let mut driver = Driver::open(audit).unwrap();
        assert_eq!(
            driver.read_fpga_boot_image().unwrap(),
            [FpgaBootImage::Golden]
        );
    }

    #[test]
    fn an_fpga_without_flash_access_is_refused_before_anything_is_sent() {
        let mut audit = Audit::new([NUM_TRANSDUCERS]);
        let fpga = audit.device_mut(0).fpga_mut();
        let functions = fpga.controller_reg(ADDR_VERSION_NUM_MAJOR);
        fpga.fpga_write(ADDR_VERSION_NUM_MAJOR, functions & 0x80FF);
        let mut driver = Driver::open(audit).unwrap();
        assert!(matches!(
            driver.update_fpga(&bitstream(10, 3), |_| {}),
            Err(DriverError::FpgaUpdateUnsupported { device: 0 })
        ));
        let audit = driver.into_link();
        assert!(audit.device(0).fpga().fpga_flash().is_empty());
    }

    #[test]
    fn output_commands_wait_for_the_reconfiguration() {
        let audit = Audit::new([NUM_TRANSDUCERS]);
        let mut driver = Driver::open(audit).unwrap();
        driver.update_fpga(&bitstream(10, 4), |_| {}).unwrap();
        assert!(matches!(
            driver.send_checked(TxFrame::new(Seq::ZERO, Cmd::Clear), DEFAULT_TIMEOUT),
            Err(DriverError::Device { code: 0x10, .. })
        ));
    }
}
