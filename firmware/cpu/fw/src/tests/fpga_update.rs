use std::vec::Vec;

use zerocopy::little_endian::{U16, U32};

use autd3_cpu_wire::fpga_update::{
    FPGA_FUNC_FLASH_OTA, FPGA_GOLDEN_REGION_END, FPGA_IMAGE_BASE, FPGA_IMAGE_CAPACITY,
    FPGA_SECTOR_BYTES, FPGA_USR_ACCESS_GOLDEN, FPGA_USR_ACCESS_UPDATE, FpgaBootImage,
};
use autd3_cpu_wire::update::crc32;

use crate::cmd::fpga_update::{
    FPGA_REBOOT_ATTEMPTS, FPGA_REBOOT_DELAY_MS, FPGA_RECONFIG_SETTLE_MS, Reconfig, State,
};
use crate::cmd::update::{UPDATE_CHUNK_MAX_DATA_LEN, UpdateBeginPayload, UpdateChunkPayload};
use crate::params::{
    ADDR_FLASH_CMD, ADDR_FLASH_LEN_0, ADDR_VERSION_NUM_MAJOR, FLASH_ERR_PROTECTED, FLASH_OP_CRC32,
    FLASH_OP_ERASE, FLASH_OP_PROGRAM, FLASH_OP_REBOOT,
};
use crate::proto::{Cmd, Error, Mode, OUTPUT_MASK_WORDS};
use crate::tests::builders::output_mask;
use crate::tests::mock::{Frame, Harness};
use autd3_cpu_wire::fpga_update::FPGA_RECONFIG_WORST_MS;

fn begin(seq: u8, length: u32, crc: u32) -> Frame {
    Frame::from_payload(
        seq,
        Cmd::FpgaUpdateBegin,
        &UpdateBeginPayload {
            length: U32::new(length),
            crc32: U32::new(crc),
        },
    )
}

fn chunk(seq: u8, offset: u32, data: &[u8]) -> Frame {
    Frame::from_parts(
        seq,
        Cmd::FpgaUpdateChunk,
        &UpdateChunkPayload {
            offset: U32::new(offset),
            data_len: U16::new(data.len() as u16),
        },
        data,
    )
}

fn image(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 8) as u8
        })
        .collect()
}

fn ota_capable() -> Harness {
    let mut h = Harness::new();
    h.set_ctl(ADDR_VERSION_NUM_MAJOR, u16::from(FPGA_FUNC_FLASH_OTA) << 8);
    h
}

fn send_image(h: &mut Harness, seq: &mut u8, img: &[u8]) {
    for (i, piece) in img.chunks(UPDATE_CHUNK_MAX_DATA_LEN).enumerate() {
        h.deliver(&chunk(*seq, (i * UPDATE_CHUNK_MAX_DATA_LEN) as u32, piece));
        assert_eq!(h.data(), 0, "chunk {i}");
        *seq = seq.wrapping_add(1);
    }
}

fn run_update(h: &mut Harness, seq: &mut u8, img: &[u8]) {
    h.deliver(&begin(*seq, img.len() as u32, crc32(img)));
    assert_eq!(h.data(), 0);
    *seq = seq.wrapping_add(1);
    send_image(h, seq, img);
    h.deliver(&Frame::new(*seq, Cmd::FpgaUpdateCommit));
    assert_eq!(h.data(), 0);
    *seq = seq.wrapping_add(1);
}

fn slot(h: &Harness, len: usize) -> &[u8] {
    let base = FPGA_IMAGE_BASE as usize;
    &h.port.fpga_flash[base..base + len]
}

fn erased_sectors(h: &Harness) -> Vec<u32> {
    h.port
        .fpga_flash_ops
        .iter()
        .filter(|(op, _, _)| *op == FLASH_OP_ERASE)
        .map(|&(_, addr, len)| {
            assert_eq!(len, FPGA_SECTOR_BYTES);
            addr
        })
        .collect()
}

fn output_muted(h: &Harness) -> bool {
    (0..OUTPUT_MASK_WORDS).all(|i| h.output_mask(i) == 0)
}

#[test]
fn an_fpga_without_flash_access_is_left_alone() {
    let mut h = Harness::new();
    h.deliver(&begin(0, 100, 0));
    assert_eq!(h.data(), Error::UpdateUnsupported as u8);
    assert!(!h.cpu.fpga_update.is_locked());
    assert!(h.port.fpga_flash_ops.is_empty());
    assert!(!output_muted(&h));
}

#[test]
fn begin_rejects_implausible_lengths() {
    let mut h = ota_capable();
    h.deliver(&begin(0, 0, 0));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&begin(1, FPGA_IMAGE_CAPACITY + 1, 0));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    assert!(!h.cpu.fpga_update.is_locked());
    assert!(h.port.fpga_flash_ops.is_empty());
}

#[test]
fn begin_mutes_the_output_and_erases_only_the_first_slot_sector() {
    let mut h = ota_capable();
    h.deliver(&begin(0, 0x30_0000, 0));
    assert_eq!(h.data(), 0);
    assert!(h.cpu.fpga_update.is_locked());
    assert!(output_muted(&h));
    assert_eq!(erased_sectors(&h), [FPGA_GOLDEN_REGION_END]);
    assert!(matches!(
        h.cpu.fpga_update.state(),
        State::Receiving {
            length: 0x30_0000,
            erased_end,
            ..
        } if erased_end == FPGA_GOLDEN_REGION_END + FPGA_SECTOR_BYTES
    ));
}

#[test]
fn a_full_update_writes_the_slot_and_erases_sectors_as_it_goes() {
    let mut h = ota_capable();
    let img = image(3 * FPGA_SECTOR_BYTES as usize, 7);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    assert_eq!(slot(&h, img.len()), &img[..]);
    assert_eq!(
        h.port.fpga_flash[FPGA_IMAGE_BASE as usize + img.len()],
        0xFF
    );
    let sectors = (FPGA_IMAGE_BASE as usize - FPGA_GOLDEN_REGION_END as usize + img.len())
        .div_ceil(FPGA_SECTOR_BYTES as usize);
    let expected: Vec<u32> = (0..sectors as u32)
        .map(|i| FPGA_GOLDEN_REGION_END + i * FPGA_SECTOR_BYTES)
        .collect();
    assert_eq!(erased_sectors(&h), expected);
    assert!(
        h.port
            .fpga_flash_ops
            .iter()
            .filter(|(op, _, _)| *op == FLASH_OP_PROGRAM)
            .all(|&(_, addr, len)| addr >= FPGA_IMAGE_BASE
                && len as usize <= UPDATE_CHUNK_MAX_DATA_LEN)
    );
    assert_eq!(
        h.port.fpga_flash_ops.last(),
        Some(&(FLASH_OP_CRC32, FPGA_IMAGE_BASE, img.len() as u32))
    );
    assert_eq!(h.cpu.fpga_update.state(), State::Committed);
    assert!(
        h.port.fpga_flash[..FPGA_GOLDEN_REGION_END as usize]
            .iter()
            .all(|&b| b == 0xFF)
    );
}

#[test]
fn a_retransmitted_chunk_is_harmless() {
    let mut h = ota_capable();
    let img = image(2000, 3);
    h.deliver(&begin(0, img.len() as u32, crc32(&img)));
    h.deliver(&chunk(1, 0, &img[..600]));
    assert_eq!(h.data(), 0);
    let erases = erased_sectors(&h).len();
    h.deliver(&chunk(2, 0, &img[..600]));
    assert_eq!(h.data(), 0);
    assert_eq!(erased_sectors(&h).len(), erases);
    h.deliver(&chunk(3, 600, &img[600..1200]));
    h.deliver(&chunk(4, 1200, &img[1200..1800]));
    h.deliver(&chunk(5, 1800, &img[1800..]));
    h.deliver(&Frame::new(6, Cmd::FpgaUpdateCommit));
    assert_eq!(h.data(), 0);
}

#[test]
fn chunks_are_validated() {
    let mut h = ota_capable();
    h.deliver(&chunk(0, 0, &[1, 2, 3]));
    assert_eq!(h.data(), Error::UpdateNotStarted as u8);
    h.deliver(&begin(1, 10, 0));
    h.deliver(&chunk(2, 8, &[1, 2, 3]));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&chunk(3, 11, &[]));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    let before = h.port.fpga_flash_ops.len();
    h.deliver(&chunk(4, 10, &[]));
    assert_eq!(h.data(), 0);
    assert_eq!(h.port.fpga_flash_ops.len(), before);
}

#[test]
fn an_odd_sized_chunk_pads_the_last_word_with_erased_bits() {
    let mut h = ota_capable();
    let img = image(5, 9);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    assert_eq!(slot(&h, 6), &[img[0], img[1], img[2], img[3], img[4], 0xFF]);
}

#[test]
fn a_crc_mismatch_keeps_the_device_locked_and_needs_a_new_session() {
    let mut h = ota_capable();
    let img = image(1000, 5);
    h.deliver(&begin(0, img.len() as u32, crc32(&img) ^ 1));
    let mut seq = 1;
    send_image(&mut h, &mut seq, &img);
    h.deliver(&Frame::new(seq, Cmd::FpgaUpdateCommit));
    assert_eq!(h.data(), Error::UpdateImageInvalid as u8);
    assert_eq!(h.cpu.fpga_update.state(), State::Idle);
    assert!(h.cpu.fpga_update.is_locked());
    h.deliver(&chunk(seq + 1, 0, &img[..10]));
    assert_eq!(h.data(), Error::UpdateNotStarted as u8);
    h.deliver(&Frame::new(seq + 2, Cmd::FpgaUpdateActivate));
    assert_eq!(h.data(), Error::UpdateNotCommitted as u8);
}

#[test]
fn commit_without_a_session_is_rejected() {
    let mut h = ota_capable();
    h.deliver(&Frame::new(0, Cmd::FpgaUpdateCommit));
    assert_eq!(h.data(), Error::UpdateNotStarted as u8);
    h.deliver(&Frame::new(1, Cmd::FpgaUpdateActivate));
    assert_eq!(h.data(), Error::UpdateNotCommitted as u8);
}

#[test]
fn output_commands_are_rejected_while_locked() {
    let mut h = ota_capable();
    h.deliver(&begin(0, 100, 0));
    let before = h.port.output_mask.clone();
    h.deliver(&output_mask(1, &[true; 249]));
    assert_eq!(h.data(), Error::FpgaUpdateInProgress as u8);
    assert_eq!(h.port.output_mask, before);
    h.deliver(&Frame::new(2, Cmd::Clear));
    assert_eq!(h.data(), Error::FpgaUpdateInProgress as u8);
    assert_eq!(h.port.output_mask, before);
    h.deliver(&Frame::new(3, Cmd::Synchronize));
    assert_eq!(h.data(), Error::FpgaUpdateInProgress as u8);
    h.deliver(&Frame::new(4, Cmd::Nop));
    assert_eq!(h.data(), 0);
    h.deliver(&Frame::new(5, Cmd::ReadCpuFwVersionMajor));
    assert_eq!(h.ack(), 5);
    h.deliver(&Frame::new(6, Cmd::ReadErrorDetail));
    assert_eq!(h.data(), Error::FpgaUpdateInProgress as u8);
    h.deliver(&Frame::new(7, Cmd::UpdateActivate));
    assert_eq!(h.data(), Error::FpgaUpdateInProgress as u8);
    assert_eq!(h.port.reset_count, 0);
    h.deliver(&begin(8, 100, 0));
    assert_eq!(h.data(), 0);
}

#[test]
fn a_floating_bus_is_not_mistaken_for_flash_support() {
    let mut h = Harness::new();
    h.set_ctl(ADDR_VERSION_NUM_MAJOR, 0xFFFF);
    h.deliver(&begin(0, 100, 0));
    assert_eq!(h.data(), Error::UpdateUnsupported as u8);
    h.deliver(&Frame::new(1, Cmd::ReadFpgaBootImage));
    assert_eq!(h.data(), FpgaBootImage::Unknown as u8);
}

#[test]
fn activation_reboots_the_fpga_and_reinitializes_it_later() {
    let mut h = ota_capable();
    let img = image(700, 11);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.deliver(&Frame::new(seq, Cmd::FpgaUpdateActivate));
    assert_eq!(h.data(), 0);
    assert_eq!(h.port.fpga_reboots, 0);

    h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) - 1);
    assert_eq!(h.port.fpga_reboots, 0);
    h.tick_1ms(1);
    assert_eq!(h.port.fpga_reboots, 1);
    assert_eq!(
        h.port.fpga_flash_ops.last().map(|op| op.0),
        Some(FLASH_OP_REBOOT)
    );
    assert_eq!(
        h.cpu.fpga_update.reconfig(),
        Reconfig::Settle(FPGA_RECONFIG_SETTLE_MS)
    );

    h.deliver(&Frame::new(seq + 1, Cmd::FpgaUpdateActivate));
    assert_eq!(h.data(), Error::FpgaUpdateInProgress as u8);
    h.deliver(&begin(seq + 2, 100, 0));
    assert_eq!(h.data(), Error::FpgaUpdateInProgress as u8);

    h.tick_1ms(u32::from(FPGA_RECONFIG_SETTLE_MS) - 1);
    assert!(h.cpu.fpga_update.is_locked());
    assert!(output_muted(&h));
    h.tick_1ms(1);
    assert!(!h.cpu.fpga_update.is_locked());
    assert_eq!(h.cpu.fpga_update.reconfig(), Reconfig::None);
    assert_eq!(h.cpu.fpga_update.state(), State::Idle);
    assert!((0..OUTPUT_MASK_WORDS).all(|i| h.output_mask(i) == 0xFFFF));
    h.tick_1ms(10_000);
    assert_eq!(h.port.fpga_reboots, 1);

    h.deliver(&Frame::new(seq + 3, Cmd::Clear));
    assert_eq!(h.data(), 0);
}

fn activated(seed: u32) -> (Harness, u8) {
    let mut h = ota_capable();
    let img = image(700, seed);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.deliver(&Frame::new(seq, Cmd::FpgaUpdateActivate));
    assert_eq!(h.data(), 0);
    (h, seq.wrapping_add(1))
}

fn reboot_requests(h: &Harness) -> usize {
    h.port
        .fpga_flash_ops
        .iter()
        .filter(|(op, _, _)| *op == FLASH_OP_REBOOT)
        .count()
}

fn error_detail(h: &mut Harness, seq: u8) -> u8 {
    h.deliver(&Frame::new(seq, Cmd::ReadErrorDetail));
    h.data()
}

#[test]
fn an_ignored_reboot_is_retried_after_the_settle_time() {
    let (mut h, seq) = activated(43);
    h.port.fpga_reboots_to_ignore = 1;
    h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS));
    assert_eq!(reboot_requests(&h), 1);
    assert_eq!(h.port.fpga_reboots, 0);
    assert!(h.cpu.fpga_update.is_locked());
    assert!(output_muted(&h));
    assert_eq!(h.cpu.fpga_update.reconfig(), Reconfig::Reboot(1));

    h.tick_1ms(1);
    assert_eq!(reboot_requests(&h), 2);
    assert_eq!(h.port.fpga_reboots, 1);
    h.tick_1ms(u32::from(FPGA_RECONFIG_SETTLE_MS));
    assert!(!h.cpu.fpga_update.is_locked());
    assert_eq!(h.cpu.fpga_update.reconfig(), Reconfig::None);
    assert!((0..OUTPUT_MASK_WORDS).all(|i| h.output_mask(i) == 0xFFFF));
    assert_ne!(error_detail(&mut h, seq), Error::FpgaReconfigFailed as u8);
}

#[test]
fn an_fpga_that_never_reconfigures_is_reported_and_released() {
    let (mut h, seq) = activated(47);
    h.port.fpga_reboots_to_ignore = u32::MAX;
    h.tick_1ms(FPGA_RECONFIG_WORST_MS - 1);
    assert_eq!(reboot_requests(&h), usize::from(FPGA_REBOOT_ATTEMPTS));
    assert!(h.cpu.fpga_update.is_locked());
    h.tick_1ms(1);
    assert!(!h.cpu.fpga_update.is_locked());
    assert_eq!(h.cpu.fpga_update.state(), State::Idle);
    assert!((0..OUTPUT_MASK_WORDS).all(|i| h.output_mask(i) == 0xFFFF));
    assert_eq!(error_detail(&mut h, seq), Error::FpgaReconfigFailed as u8);
    h.tick_1ms(10_000);
    assert_eq!(reboot_requests(&h), usize::from(FPGA_REBOOT_ATTEMPTS));
    assert_eq!(h.port.fpga_reboots, 0);
}

#[test]
fn a_reboot_write_ignored_while_busy_is_detected() {
    let (mut h, seq) = activated(53);
    h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) - 1);
    h.port.fpga_flash_dropped_reg = Some(ADDR_FLASH_CMD);
    h.tick_1ms(1 + u32::from(FPGA_RECONFIG_SETTLE_MS));
    assert_eq!(h.port.fpga_reboots, 0);
    assert_eq!(h.cpu.fpga_update.reconfig(), Reconfig::Reboot(1));
    h.port.fpga_flash_dropped_reg = None;
    h.tick_1ms(1 + u32::from(FPGA_RECONFIG_SETTLE_MS));
    assert_eq!(h.port.fpga_reboots, 1);
    assert!(!h.cpu.fpga_update.is_locked());
    assert_ne!(error_detail(&mut h, seq), Error::FpgaReconfigFailed as u8);
}

#[test]
fn flash_errors_surface_as_update_flash() {
    let mut h = ota_capable();
    h.port.fpga_flash_err = Some(FLASH_ERR_PROTECTED);
    h.deliver(&begin(0, 100, 0));
    assert_eq!(h.data(), Error::UpdateFlash as u8);
    assert_eq!(h.cpu.fpga_update.state(), State::Idle);
    assert!(h.cpu.fpga_update.is_locked());
    assert!(output_muted(&h));
    h.deliver(&Frame::new(1, Cmd::Clear));
    assert_eq!(h.data(), Error::FpgaUpdateInProgress as u8);
}

#[test]
fn an_erase_failure_mid_chunk_keeps_the_session_and_the_erased_range() {
    let mut h = ota_capable();
    let img = image(FPGA_SECTOR_BYTES as usize + 100, 36);
    h.deliver(&begin(0, img.len() as u32, crc32(&img)));
    assert_eq!(h.data(), 0);
    let State::Receiving { erased_end, .. } = h.cpu.fpga_update.state() else {
        panic!("session did not open");
    };
    h.port.fpga_flash_err = Some(FLASH_ERR_PROTECTED);
    h.deliver(&chunk(
        1,
        FPGA_SECTOR_BYTES,
        &img[FPGA_SECTOR_BYTES as usize..],
    ));
    assert_eq!(h.data(), Error::UpdateFlash as u8);
    assert_eq!(erased_sectors(&h).len(), 2);
    assert_eq!(
        h.cpu.fpga_update.state(),
        State::Receiving {
            length: img.len() as u32,
            crc32: crc32(&img),
            erased_end,
        }
    );
    assert!(h.cpu.fpga_update.is_locked());
}

#[test]
fn activate_is_deferred_even_in_low_latency_mode() {
    let mut h = ota_capable();
    let img = image(700, 37);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.cpu.set_mode(Mode::LowLatency);
    h.deliver_no_drain(&Frame::new(seq, Cmd::FpgaUpdateActivate));
    assert_eq!(h.ack(), seq.wrapping_sub(1));
    assert_eq!(h.cpu.fpga_update.state(), State::Committed);
    assert!(h.process_one());
    assert_eq!(h.ack(), seq);
    assert_eq!(h.data(), 0);
    assert!(!h.process_one());
    h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS));
    assert_eq!(h.port.fpga_reboots, 1);
}

#[test]
fn every_flash_command_is_issued_after_its_target_is_flushed() {
    let mut h = ota_capable();
    let img = image(3 * FPGA_SECTOR_BYTES as usize, 41);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.deliver(&Frame::new(seq, Cmd::FpgaUpdateActivate));
    h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS));
    assert_eq!(h.port.fpga_reboots, 1);
    assert!(h.port.fpga_flash_ops.len() > 3);
    assert_eq!(h.port.fpga_flash_cmds_before_flush, 0);
}

#[test]
fn a_lost_target_write_stops_the_command() {
    let mut h = ota_capable();
    h.deliver(&begin(0, 100, 0));
    assert_eq!(h.data(), 0);
    let ops = h.port.fpga_flash_ops.len();
    h.port.fpga_flash_dropped_reg = Some(ADDR_FLASH_LEN_0);
    h.deliver(&chunk(1, 0, &[0xAB; 16]));
    assert_eq!(h.data(), Error::UpdateFlash as u8);
    assert_eq!(h.port.fpga_flash_ops.len(), ops);
}

#[test]
fn a_hung_fpga_times_out() {
    let mut h = ota_capable();
    h.port.fpga_flash_hang = true;
    h.deliver(&begin(0, 100, 0));
    assert_eq!(h.data(), Error::FpgaTimeout as u8);
    assert_eq!(h.cpu.fpga_update.state(), State::Idle);
    let ops = h.port.fpga_flash_ops.len();
    h.deliver(&begin(1, 100, 0));
    assert_eq!(h.data(), Error::FpgaTimeout as u8);
    assert_eq!(h.port.fpga_flash_ops.len(), ops);
}

#[test]
fn a_chunk_erases_every_sector_it_reaches() {
    let mut h = ota_capable();
    let length = 3 * FPGA_SECTOR_BYTES;
    h.deliver(&begin(0, length, 0));
    let far = 2 * FPGA_SECTOR_BYTES + 10;
    h.deliver(&chunk(1, far, &[0xAB; 16]));
    assert_eq!(h.data(), 0);
    assert_eq!(
        erased_sectors(&h),
        [
            FPGA_GOLDEN_REGION_END,
            FPGA_GOLDEN_REGION_END + FPGA_SECTOR_BYTES,
            FPGA_GOLDEN_REGION_END + 2 * FPGA_SECTOR_BYTES
        ]
    );
    h.deliver(&chunk(2, 0, &[0xCD; 16]));
    assert_eq!(erased_sectors(&h).len(), 3);
}

#[test]
fn boot_image_reports_the_usr_access_value() {
    let mut h = Harness::new();
    h.port.fpga_usr_access = FPGA_USR_ACCESS_UPDATE;
    h.deliver(&Frame::new(0, Cmd::ReadFpgaBootImage));
    assert_eq!(h.data(), FpgaBootImage::Unknown as u8);

    h.set_ctl(ADDR_VERSION_NUM_MAJOR, u16::from(FPGA_FUNC_FLASH_OTA) << 8);
    h.deliver(&Frame::new(1, Cmd::ReadFpgaBootImage));
    assert_eq!(h.data(), FpgaBootImage::Update as u8);
    h.port.fpga_usr_access = FPGA_USR_ACCESS_GOLDEN;
    h.deliver(&Frame::new(2, Cmd::ReadFpgaBootImage));
    assert_eq!(h.data(), FpgaBootImage::Golden as u8);
    h.port.fpga_usr_access = 0xFFFF_FFFF;
    h.deliver(&Frame::new(3, Cmd::ReadFpgaBootImage));
    assert_eq!(h.data(), FpgaBootImage::Unknown as u8);
}

#[test]
fn fpga_update_commands_are_deferred_even_in_low_latency_mode() {
    let mut h = ota_capable();
    h.cpu.set_mode(Mode::LowLatency);
    h.deliver_no_drain(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);
    h.deliver_no_drain(&begin(1, 100, 0));
    assert_eq!(h.ack(), 0);
    assert!(h.port.fpga_flash_ops.is_empty());
    assert!(h.process_one());
    assert_eq!(h.ack(), 1);
    assert_eq!(erased_sectors(&h).len(), 1);
    assert!(!h.process_one());
}

#[test]
fn a_cpu_boot_clears_the_fpga_session() {
    let mut h = ota_capable();
    h.deliver(&begin(0, 100, 0));
    assert!(h.cpu.fpga_update.is_locked());
    h.init();
    assert!(!h.cpu.fpga_update.is_locked());
    assert_eq!(h.cpu.fpga_update.state(), State::Idle);
}
