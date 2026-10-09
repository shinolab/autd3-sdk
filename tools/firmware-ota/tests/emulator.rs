use autd3_cpu_wire::update::{ImageHeader, RunningImage, Slot, crc32};
use std::time::Duration;

use autd3_rs::protocol::{Cmd, Seq};
use autd3_rs_firmware_emulator::EMULATED_CPU_IMAGE;
use autd3_rs_firmware_emulator::test_utils::{Audit, DeviceTestExt};
use autd3_rs_firmware_ota::{
    CpuFirmwareImage, DeviceReply, Driver, DriverError, Exchange, Frame, Replies, UpdateProgress,
};
use zerocopy::FromBytes;

struct Chain(Audit);

impl From<Audit> for Chain {
    fn from(audit: Audit) -> Self {
        Self(audit)
    }
}

impl core::ops::Deref for Chain {
    type Target = Audit;

    fn deref(&self) -> &Audit {
        &self.0
    }
}

impl core::ops::DerefMut for Chain {
    fn deref_mut(&mut self) -> &mut Audit {
        &mut self.0
    }
}

impl Exchange for Chain {
    type Error = core::convert::Infallible;

    fn num_devices(&self) -> usize {
        self.0.num_devices()
    }

    fn reset(&mut self, _timeout: Duration) -> Result<bool, Self::Error> {
        let reset = [0, Cmd::Reset.as_u8()];
        let frames = vec![&reset[..]; self.0.num_devices()];
        let replies = self.0.send(&frames, 0);
        Ok(replies.len() == self.0.num_devices())
    }

    fn exchange(
        &mut self,
        seq: Seq,
        frame: &Frame,
        _timeout: Duration,
        _retransmit: Duration,
    ) -> Result<Replies, Self::Error> {
        let bytes = frame.bytes(seq);
        let frames = vec![&bytes[..]; self.0.num_devices()];
        let mut replies: Vec<Option<DeviceReply>> = vec![None; self.0.num_devices()];
        for r in self.0.send(&frames, 0) {
            if r.reply.ack == seq.get() {
                replies[r.device] = Some(DeviceReply {
                    status: r.reply.status.as_u8(),
                    value: r.reply.data().to_vec(),
                });
            }
        }
        Ok(match replies.iter().position(Option::is_none) {
            Some(device) => Err(device),
            None => Ok(replies.into_iter().map(Option::unwrap).collect()),
        })
    }

    fn idle(&mut self, _duration: Duration) -> Result<(), Self::Error> {
        Ok(())
    }
}

const NUM_TRANSDUCERS: usize = 249;

fn image(len: usize, seed: u8) -> CpuFirmwareImage {
    let body: Vec<u8> = (0..len)
        .map(|i| i.to_le_bytes()[0].wrapping_mul(31).wrapping_add(seed))
        .collect();
    CpuFirmwareImage::from_slot_image(body).unwrap()
}

fn header(audit: &Audit, device: usize, slot: Slot) -> ImageHeader {
    let flash = audit.device(device).fpga().cpu_flash();
    let base = slot.base() as usize;
    ImageHeader::read_from_bytes(&flash[base..][..core::mem::size_of::<ImageHeader>()]).unwrap()
}

fn body(audit: &Audit, device: usize, slot: Slot, len: usize) -> Vec<u8> {
    let base = slot.image_base() as usize;
    audit.device(device).fpga().cpu_flash()[base..][..len].to_vec()
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
    let mut driver = Driver::open(Chain::from(audit)).unwrap();
    assert_eq!(driver.num_devices(), 2);
    assert_eq!(driver.read_firmware_info().unwrap().len(), 2);

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

    let mut audit = driver.into_inner();
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

    let mut audit = driver.into_inner();
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
    let mut driver = Driver::open(Chain::from(audit)).unwrap();
    let next = image(5000, 7);
    driver.update(&next, |_| {}).unwrap();
    driver.activate().unwrap();

    let mut audit = driver.into_inner();
    reboot_all(&mut audit, 1);
    assert_eq!(audit.device(0).booted_slot(), Some(Slot::B));

    let mut driver = Driver::open(audit).unwrap();
    assert_eq!(driver.read_firmware_info().unwrap().len(), 1);
    let mut audit = driver.into_inner();

    audit.device_mut(0).power_cycle();
    assert_eq!(audit.device(0).booted_slot(), Some(Slot::A));
    assert_eq!(
        body(&audit, 0, Slot::A, EMULATED_CPU_IMAGE.len()),
        EMULATED_CPU_IMAGE
    );

    let mut driver = Driver::open(audit).unwrap();
    driver.confirm().unwrap();
    let mut audit = driver.into_inner();
    audit.device_mut(0).power_cycle();
    assert_eq!(audit.device(0).booted_slot(), Some(Slot::A));
    assert!(header(&audit, 0, Slot::B).is_trial());
}

#[test]
fn the_running_image_tells_a_unit_on_trial_from_one_that_rolled_back() {
    const TRIAL: Option<RunningImage> = Some(RunningImage::Unconfirmed);
    const CONFIRMED: Option<RunningImage> = Some(RunningImage::Confirmed);

    let audit = Audit::new([NUM_TRANSDUCERS, NUM_TRANSDUCERS]);
    let mut driver = Driver::open(Chain::from(audit)).unwrap();
    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
    driver.update(&image(5000, 9), |_| {}).unwrap();
    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
    driver.activate().unwrap();

    let mut audit = driver.into_inner();
    reboot_all(&mut audit, 2);
    let mut driver = Driver::open(audit).unwrap();
    assert_eq!(driver.read_running_image().unwrap(), [TRIAL; 2]);

    let mut audit = driver.into_inner();
    audit.device_mut(1).power_cycle();
    assert_eq!(audit.device(1).booted_slot(), Some(Slot::A));
    let mut driver = Driver::open(audit).unwrap();
    assert_eq!(driver.read_running_image().unwrap(), [TRIAL, CONFIRMED]);

    driver.confirm().unwrap();
    assert_eq!(driver.read_running_image().unwrap(), [CONFIRMED; 2]);
}

#[test]
fn reboot_resets_every_device_without_touching_the_flash() {
    let audit = Audit::new([NUM_TRANSDUCERS, NUM_TRANSDUCERS]);
    let before: Vec<Vec<u8>> = (0..2)
        .map(|device| audit.device(device).fpga().cpu_flash().to_vec())
        .collect();
    let mut driver = Driver::open(Chain::from(audit)).unwrap();
    driver.reboot().unwrap();

    let mut audit = driver.into_inner();
    reboot_all(&mut audit, 2);
    for (device, flash) in before.iter().enumerate() {
        assert_eq!(audit.device(device).booted_slot(), Some(Slot::A));
        assert_eq!(audit.device(device).fpga().cpu_flash(), &flash[..]);
    }
}

#[test]
fn confirm_before_any_update_is_a_no_op_on_a_normal_image() {
    let audit = Audit::new([NUM_TRANSDUCERS]);
    let before = audit.device(0).fpga().cpu_flash().to_vec();
    let mut driver = Driver::open(Chain::from(audit)).unwrap();
    driver.confirm().unwrap();
    let audit = driver.into_inner();
    assert_eq!(audit.device(0).fpga().cpu_flash(), &before[..]);
}

#[test]
fn update_refuses_when_the_device_has_no_valid_slot() {
    let mut audit = Audit::new([NUM_TRANSDUCERS]);
    audit.device_mut(0).fpga_mut().cpu_flash_mut().fill(0xFF);
    let mut driver = Driver::open(Chain::from(audit)).unwrap();
    assert!(matches!(
        driver.update(&image(100, 3), |_| {}),
        Err(DriverError::Device {
            device: 0,
            code: 0x0C,
            ..
        })
    ));
    let audit = driver.into_inner();
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
    let mut driver = Driver::open(Chain::from(audit)).unwrap();
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
    use autd3_rs::protocol::Cmd;
    use autd3_rs_firmware_emulator::test_utils::{Audit, FpgaEmulatorTestExt};

    use super::Chain;
    use autd3_rs_firmware_emulator::autd3_cpu_fw::Port;
    use autd3_rs_firmware_emulator::autd3_cpu_fw::fpga_params::ADDR_FUNCTION_BITS;
    use autd3_rs_firmware_ota::{
        DEFAULT_TIMEOUT, Driver, DriverError, FpgaFirmwareImage, Frame, boot_image,
    };

    use super::NUM_TRANSDUCERS;

    const RECONFIG_TICKS: usize = 3200;

    fn boot_images(driver: &mut Driver<Chain>) -> Vec<FpgaBootImage> {
        driver
            .read_firmware_info()
            .unwrap()
            .iter()
            .map(boot_image)
            .collect()
    }

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
        audit.device(device).fpga().fpga_flash()[base..][..len].to_vec()
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
        let mut driver = Driver::open(Chain::from(audit)).unwrap();
        assert_eq!(
            boot_images(&mut driver),
            [FpgaBootImage::Update, FpgaBootImage::Update]
        );
        let image = bitstream(40_000, 1);
        let mut last = None;
        driver.update_fpga(&image, |p| last = Some(p)).unwrap();
        assert_eq!(last.map(|p| p.sent), Some(image.len()));
        driver.activate_fpga().unwrap();

        let mut audit = driver.into_inner();
        for device in 0..2 {
            assert_eq!(slot(&audit, device, image.len()), image.as_bytes());
            assert!(
                audit.device(device).fpga().fpga_flash()[..FPGA_IMAGE_BASE as usize]
                    .iter()
                    .all(|&b| b == 0xFF)
            );
            assert!(audit.device(device).fpga().failsafe());
        }
        settle(&mut audit, 2);
        for device in 0..2 {
            assert_eq!(audit.device(device).fpga().reconfig_count(), 1);
            assert_eq!(audit.device(device).fpga().reset_count(), 0);
            assert!(!audit.device(device).fpga().failsafe());
        }

        let mut driver = Driver::open(audit).unwrap();
        assert_eq!(
            boot_images(&mut driver),
            [FpgaBootImage::Update, FpgaBootImage::Update]
        );
        assert_eq!(driver.read_firmware_info().unwrap().len(), 2);
        driver.close().unwrap();
    }

    #[test]
    fn an_fpga_that_ignores_reboot_is_reported() {
        let mut audit = Audit::new([NUM_TRANSDUCERS, NUM_TRANSDUCERS]);
        audit.device_mut(1).fpga_mut().ignore_next_reboots(u32::MAX);
        let mut driver = Driver::open(Chain::from(audit)).unwrap();
        let image = bitstream(1000, 4);
        driver.update_fpga(&image, |_| {}).unwrap();
        driver.activate_fpga().unwrap();
        let mut audit = driver.into_inner();
        for _ in 0..3 {
            settle(&mut audit, 2);
        }
        assert_eq!(audit.device(0).fpga().reconfig_count(), 1);
        assert_eq!(audit.device(1).fpga().reconfig_count(), 0);
        assert!(!audit.device(1).fpga().failsafe());

        let mut driver = Driver::open(audit).unwrap();
        assert_eq!(
            boot_images(&mut driver),
            [FpgaBootImage::Update, FpgaBootImage::ReconfigFailed]
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
        let mut driver = Driver::open(Chain::from(audit)).unwrap();
        driver.update_fpga(&bitstream(1000, 5), |_| {}).unwrap();
        driver.activate_fpga().unwrap();
        let mut audit = driver.into_inner();
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
        let mut driver = Driver::open(Chain::from(audit)).unwrap();
        let image = bitstream(1000, 2);
        driver.update_fpga(&image, |_| {}).unwrap();
        driver.activate_fpga().unwrap();
        let mut audit = driver.into_inner();
        let base = FPGA_IMAGE_BASE as usize;
        audit.device_mut(0).fpga_mut().fpga_flash_mut()[base..][..32].fill(0xFF);
        settle(&mut audit, 1);
        let mut driver = Driver::open(audit).unwrap();
        assert_eq!(boot_images(&mut driver), [FpgaBootImage::Golden]);
    }

    #[test]
    fn an_fpga_without_flash_access_is_refused_before_anything_is_sent() {
        let mut audit = Audit::new([NUM_TRANSDUCERS]);
        let fpga = audit.device_mut(0).fpga_mut();
        let functions = fpga.controller_reg(ADDR_FUNCTION_BITS);
        fpga.fpga_write(ADDR_FUNCTION_BITS, functions & 0x0080);
        let mut driver = Driver::open(Chain::from(audit)).unwrap();
        assert!(matches!(
            driver.update_fpga(&bitstream(10, 3), |_| {}),
            Err(DriverError::FpgaUpdateUnsupported { device: 0 })
        ));
        let audit = driver.into_inner();
        assert_eq!(audit.device(0).fpga().fpga_flash(), [0u8; 0]);
    }

    #[test]
    fn output_commands_wait_for_the_reconfiguration() {
        let audit = Audit::new([NUM_TRANSDUCERS]);
        let mut driver = Driver::open(Chain::from(audit)).unwrap();
        driver.update_fpga(&bitstream(10, 4), |_| {}).unwrap();
        assert!(matches!(
            driver.send_checked(&Frame::new(Cmd::Clear), DEFAULT_TIMEOUT),
            Err(DriverError::Device { code: 0x10, .. })
        ));
    }
}

mod udp {
    use std::time::Duration;

    use autd3_cpu_wire::update::Slot;
    use autd3_rs::{TransportOption, UdpBus};
    use autd3_rs_firmware_emulator::test_utils::DeviceTestExt;
    use autd3_rs_firmware_emulator::udp::UdpEmulator;
    use autd3_rs_firmware_ota::{Driver, DriverError};

    use super::image;

    const RESET_DELAY_TICKS: usize = 100;

    fn open(emulator: &UdpEmulator) -> Driver<UdpBus> {
        let option = TransportOption {
            iface: emulator.interface(),
            enumeration_timeout: Duration::from_secs(1),
            ..TransportOption::default()
        };
        Driver::open(UdpBus::open_unsynchronized(&option, emulator.num_devices()).unwrap()).unwrap()
    }

    fn committed(emulator: &UdpEmulator) -> Driver<UdpBus> {
        let mut driver = open(emulator);
        driver.update(&image(3000, 9), |_| {}).unwrap();
        driver
    }

    fn resets_after_the_delay(emulator: &UdpEmulator) -> Vec<u32> {
        (0..emulator.num_devices())
            .map(|device| {
                emulator.with_device(device, |d| {
                    for _ in 0..RESET_DELAY_TICKS {
                        d.tick_1ms();
                    }
                    d.fpga().reset_count()
                })
            })
            .collect()
    }

    fn booted_slots(emulator: &UdpEmulator) -> Vec<Option<Slot>> {
        (0..emulator.num_devices())
            .map(|device| emulator.with_device(device, |d| d.booted_slot()))
            .collect()
    }

    #[test]
    fn activation_reaches_a_unit_behind_one_that_is_about_to_reset() {
        for dropped in [1, 2] {
            let emulator = UdpEmulator::spawn(2).unwrap();
            let mut driver = committed(&emulator);
            emulator.drop_next_frames(1, dropped);
            driver.activate().unwrap();
            driver.close().unwrap();
            assert_eq!(resets_after_the_delay(&emulator), [1, 1]);
            assert_eq!(booted_slots(&emulator), [Some(Slot::B), Some(Slot::B)]);
        }
    }

    #[test]
    fn activation_survives_a_lost_reply() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let mut driver = committed(&emulator);
        emulator.drop_next_replies(0, 1);
        driver.activate().unwrap();
        driver.close().unwrap();
        assert_eq!(resets_after_the_delay(&emulator), [1, 1]);
        assert_eq!(booted_slots(&emulator), [Some(Slot::B), Some(Slot::B)]);
    }

    #[test]
    fn reboot_reaches_a_unit_behind_one_that_is_about_to_reset() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let mut driver = open(&emulator);
        emulator.drop_next_frames(1, 2);
        driver.reboot().unwrap();
        driver.close().unwrap();
        assert_eq!(resets_after_the_delay(&emulator), [1, 1]);
        assert_eq!(booted_slots(&emulator), [Some(Slot::A), Some(Slot::A)]);
    }

    #[test]
    fn an_unanswered_activation_points_at_confirm_only() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let mut driver = committed(&emulator);
        emulator.drop_next_frames(1, u32::MAX);
        let err = driver.activate().unwrap_err();
        assert!(matches!(err, DriverError::Timeout { device: 1, .. }));
        let message = err.to_string();
        assert!(message.contains("`--confirm-only`"), "{message}");
        assert!(!message.contains("firewall"), "{message}");
        emulator.drop_next_frames(1, 0);
        assert_eq!(resets_after_the_delay(&emulator), [1, 0]);
    }

    #[test]
    fn the_units_that_missed_the_activation_are_activated_after_the_reconnection() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let mut driver = committed(&emulator);
        emulator.drop_next_frames(1, u32::MAX);
        assert!(driver.activate().is_err());
        driver.close().unwrap();
        emulator.drop_next_frames(1, 0);
        assert_eq!(resets_after_the_delay(&emulator), [1, 0]);
        assert_eq!(booted_slots(&emulator), [Some(Slot::B), Some(Slot::A)]);

        let mut driver = open(&emulator);
        assert_eq!(driver.activate_unrebooted().unwrap(), [1]);
        driver.close().unwrap();
        assert_eq!(resets_after_the_delay(&emulator), [1, 1]);
        assert_eq!(booted_slots(&emulator), [Some(Slot::B), Some(Slot::B)]);

        let mut driver = open(&emulator);
        assert_eq!(driver.activate_unrebooted().unwrap(), [0usize; 0]);
        driver.confirm().unwrap();
        driver.close().unwrap();
    }
}
