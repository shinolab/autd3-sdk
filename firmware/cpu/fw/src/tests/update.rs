use std::vec::Vec;

use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, IntoBytes};

use autd3_cpu_wire::update::{
    FLASH_SECTOR_BYTES, IMAGE_APP_CAPACITY, IMAGE_VECTOR_BYTES, ImageHeader, LOADER_REGION_END,
    SLOT_HEADER_BYTES, SLOT_IMAGE_CAPACITY, Slot, crc32,
};

use crate::cmd::update::{
    ACTIVATE_DELAY_MS, State, UPDATE_CHUNK_MAX_DATA_LEN, UpdateBeginPayload, UpdateChunkPayload,
};
use crate::proto::{Cmd, Error, Mode};
use crate::tests::mock::{Frame, Harness};

const HEADER_BYTES: usize = core::mem::size_of::<ImageHeader>();

fn begin(seq: u8, length: u32, crc: u32) -> Frame {
    Frame::from_payload(
        seq,
        Cmd::UpdateBegin,
        &UpdateBeginPayload {
            length: U32::new(length),
            crc32: U32::new(crc),
        },
    )
}

fn chunk(seq: u8, offset: u32, data: &[u8]) -> Frame {
    Frame::from_parts(
        seq,
        Cmd::UpdateChunk,
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
    h.deliver(&Frame::new(*seq, Cmd::UpdateCommit));
    assert_eq!(h.data(), 0);
    *seq = seq.wrapping_add(1);
}

fn header_of(h: &Harness, slot: Slot) -> ImageHeader {
    let base = slot.base() as usize;
    ImageHeader::read_from_bytes(&h.port.flash[base..base + HEADER_BYTES]).unwrap()
}

fn slot_image(h: &Harness, slot: Slot, len: usize) -> &[u8] {
    let base = slot.image_base() as usize;
    &h.port.flash[base..base + len]
}

fn stamp(h: &mut Harness, slot: Slot, generation: u32, img: &[u8]) {
    let header = ImageHeader::new(generation, img.len() as u32, crc32(img));
    let base = slot.base() as usize;
    h.port.flash[base..base + HEADER_BYTES].copy_from_slice(header.as_bytes());
    let image_base = slot.image_base() as usize;
    h.port.flash[image_base..image_base + img.len()].copy_from_slice(img);
}

const RUNNING_IMAGE_LEN: usize = 256;

fn running_from_slot_a() -> (Harness, Vec<u8>) {
    let mut h = Harness::new();
    let running = image(RUNNING_IMAGE_LEN, 0xA0);
    stamp(&mut h, Slot::A, 0, &running);
    (h, running)
}

fn erased_len(len: u32) -> u32 {
    (SLOT_HEADER_BYTES + len).div_ceil(FLASH_SECTOR_BYTES) * FLASH_SECTOR_BYTES
}

#[test]
fn begin_refuses_to_erase_when_no_slot_is_valid() {
    let mut h = Harness::new();
    h.deliver(&begin(0, 5000, 0));
    assert_eq!(h.data(), Error::UpdateFlash as u8);
    assert_eq!(h.cpu.update.state(), State::Idle);
    assert!(h.port.erased.is_empty());
    assert!(h.port.flash.iter().all(|&b| b == 0xFF));

    h.deliver(&chunk(1, 0, &[1, 2, 3]));
    assert_eq!(h.data(), Error::UpdateNotStarted as u8);
}

#[test]
fn begin_refuses_when_the_only_header_has_a_wrong_crc() {
    let (mut h, _) = running_from_slot_a();
    let corrupt = Slot::A.image_base() as usize + 3;
    h.port.flash[corrupt] ^= 0x10;
    h.deliver(&begin(0, 5000, 0));
    assert_eq!(h.data(), Error::UpdateFlash as u8);
    assert!(h.port.erased.is_empty());
}

#[test]
fn first_update_on_a_fresh_device_targets_slot_b_with_generation_1() {
    let (mut h, running) = running_from_slot_a();
    let img = image(5000, 1);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);

    let header = header_of(&h, Slot::B);
    assert!(header.is_plausible());
    assert_eq!(header.generation.get(), 1);
    assert_eq!(header.length.get(), img.len() as u32);
    assert_eq!(header.crc32.get(), crc32(&img));
    assert_eq!(slot_image(&h, Slot::B, img.len()), &img[..]);
    assert_eq!(header_of(&h, Slot::A).generation.get(), 0);
    assert_eq!(slot_image(&h, Slot::A, running.len()), &running[..]);
    assert_eq!(h.cpu.update.state(), State::Committed);
    assert_eq!(
        h.port.erased,
        [(Slot::B.base(), erased_len(img.len() as u32))]
    );
}

#[test]
fn update_writes_the_inactive_slot_with_the_next_generation() {
    let mut h = Harness::new();
    let running = image(3000, 7);
    stamp(&mut h, Slot::A, 3, &running);
    let img = image(70_000, 2);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);

    let b = header_of(&h, Slot::B);
    assert_eq!(b.generation.get(), 4);
    assert_eq!(slot_image(&h, Slot::B, img.len()), &img[..]);
    assert_eq!(header_of(&h, Slot::A).generation.get(), 3);
    assert_eq!(slot_image(&h, Slot::A, running.len()), &running[..]);
    assert!(
        h.port.flash[..LOADER_REGION_END as usize]
            .iter()
            .all(|&b| b == 0xFF)
    );
}

#[test]
fn newest_generation_decides_the_current_slot() {
    let mut h = Harness::new();
    stamp(&mut h, Slot::A, 5, &image(100, 3));
    stamp(&mut h, Slot::B, 7, &image(100, 4));
    let img = image(1000, 5);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    assert_eq!(header_of(&h, Slot::A).generation.get(), 8);
    assert_eq!(header_of(&h, Slot::B).generation.get(), 7);
}

#[test]
fn a_slot_whose_image_crc_is_wrong_is_not_current() {
    let mut h = Harness::new();
    stamp(&mut h, Slot::A, 1, &image(100, 3));
    stamp(&mut h, Slot::B, 9, &image(100, 4));
    let corrupt = Slot::B.image_base() as usize + 10;
    h.port.flash[corrupt] ^= 0x01;
    let img = image(1000, 5);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    assert_eq!(header_of(&h, Slot::B).generation.get(), 2);
    assert_eq!(header_of(&h, Slot::A).generation.get(), 1);
}

#[test]
fn commit_rejects_a_crc_mismatch_and_leaves_the_slot_invalid() {
    let (mut h, _) = running_from_slot_a();
    let img = image(2000, 6);
    h.deliver(&begin(0, img.len() as u32, crc32(&img) ^ 1));
    let mut seq = 1;
    send_image(&mut h, &mut seq, &img);
    h.deliver(&Frame::new(seq, Cmd::UpdateCommit));
    assert_eq!(h.data(), Error::UpdateImageInvalid as u8);
    assert!(!header_of(&h, Slot::B).is_plausible());
    assert_eq!(header_of(&h, Slot::A).generation.get(), 0);
    assert_eq!(h.cpu.update.state(), State::Idle);

    h.deliver(&chunk(seq + 1, 0, &img[..8]));
    assert_eq!(h.data(), Error::UpdateNotStarted as u8);
    h.deliver(&Frame::new(seq + 2, Cmd::UpdateCommit));
    assert_eq!(h.data(), Error::UpdateNotStarted as u8);
    h.deliver(&Frame::new(seq + 3, Cmd::UpdateActivate));
    assert_eq!(h.data(), Error::UpdateNotCommitted as u8);
    h.deliver(&Frame::new(seq + 4, Cmd::ReadErrorDetail));
    assert_eq!(h.data(), Error::UpdateNotCommitted as u8);
}

#[test]
fn chunk_and_commit_without_begin_are_rejected() {
    let mut h = Harness::new();
    h.deliver(&chunk(0, 0, &[1, 2, 3]));
    assert_eq!(h.data(), Error::UpdateNotStarted as u8);
    h.deliver(&Frame::new(1, Cmd::UpdateCommit));
    assert_eq!(h.data(), Error::UpdateNotStarted as u8);
    assert!(h.port.erased.is_empty());
    assert!(h.port.flash.iter().all(|&b| b == 0xFF));
}

#[test]
fn chunk_out_of_range_is_invalid_payload() {
    let (mut h, _) = running_from_slot_a();
    let img = image(1000, 8);
    h.deliver(&begin(0, img.len() as u32, crc32(&img)));
    h.deliver(&chunk(1, 996, &img[..8]));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&chunk(2, 1001, &[]));
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    let oversized = Frame::from_parts(
        3,
        Cmd::UpdateChunk,
        &UpdateChunkPayload {
            offset: U32::new(0),
            data_len: U16::new((UPDATE_CHUNK_MAX_DATA_LEN + 1) as u16),
        },
        &[],
    );
    h.deliver(&oversized);
    assert_eq!(h.data(), Error::InvalidPayload as u8);
    h.deliver(&chunk(4, 1000, &[]));
    assert_eq!(h.data(), 0);
    assert_eq!(
        h.cpu.update.state(),
        State::Receiving {
            slot: Slot::B,
            length: 1000,
            crc32: crc32(&img),
            generation: 1
        }
    );
}

#[test]
fn begin_rejects_lengths_the_loader_cannot_copy() {
    let max = IMAGE_VECTOR_BYTES + IMAGE_APP_CAPACITY;
    let (mut h, _) = running_from_slot_a();
    for (seq, length) in [
        0,
        IMAGE_VECTOR_BYTES,
        max + 1,
        SLOT_IMAGE_CAPACITY,
        SLOT_IMAGE_CAPACITY + 1,
    ]
    .into_iter()
    .enumerate()
    {
        h.deliver(&begin(seq as u8, length, 0));
        assert_eq!(h.data(), Error::InvalidPayload as u8, "length {length}");
    }
    assert_eq!(h.cpu.update.state(), State::Idle);
    assert!(h.port.erased.is_empty());
    h.deliver(&begin(5, max, 0));
    assert_eq!(h.data(), 0);
    assert_eq!(h.port.erased, [(Slot::B.base(), erased_len(max))]);
}

#[test]
fn activate_requires_a_commit_and_resets_after_the_delay() {
    let (mut h, _) = running_from_slot_a();
    let img = image(300, 9);
    let mut seq = 0;
    h.deliver(&begin(seq, img.len() as u32, crc32(&img)));
    seq += 1;
    h.deliver(&Frame::new(seq, Cmd::UpdateActivate));
    assert_eq!(h.data(), Error::UpdateNotCommitted as u8);
    seq += 1;
    send_image(&mut h, &mut seq, &img);
    h.deliver(&Frame::new(seq, Cmd::UpdateCommit));
    assert_eq!(h.data(), 0);
    seq += 1;
    h.deliver(&Frame::new(seq, Cmd::UpdateActivate));
    assert_eq!(h.data(), 0);
    assert_eq!(h.cpu.update.reset_countdown(), ACTIVATE_DELAY_MS);

    h.tick_1ms(u32::from(ACTIVATE_DELAY_MS) - 1);
    assert_eq!(h.port.reset_count, 0);
    h.tick_1ms(1);
    assert_eq!(h.port.reset_count, 1);
    h.tick_1ms(10);
    assert_eq!(h.port.reset_count, 1);
}

#[test]
fn flash_driver_failure_is_reported() {
    let (mut h, _) = running_from_slot_a();
    h.port.flash_fail = true;
    h.deliver(&begin(0, 100, 0));
    assert_eq!(h.data(), Error::UpdateFlash as u8);
    assert_eq!(h.cpu.update.state(), State::Idle);
}

#[test]
fn update_commands_are_deferred_even_in_low_latency_mode() {
    let (mut h, _) = running_from_slot_a();
    h.cpu.set_mode(Mode::LowLatency);
    h.deliver_no_drain(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);

    let img = image(100, 10);
    h.deliver_no_drain(&begin(1, img.len() as u32, crc32(&img)));
    assert_eq!(h.ack(), 0);
    assert!(h.port.erased.is_empty());
    h.deliver_no_drain(&Frame::new(2, Cmd::Nop));
    assert_eq!(h.ack(), 0);

    assert!(h.process_one());
    assert_eq!(h.ack(), 1);
    assert_eq!(h.port.erased.len(), 1);
    assert!(h.process_one());
    assert_eq!(h.ack(), 2);
    assert!(!h.process_one());
}

#[test]
fn protocol_reset_keeps_the_session_open() {
    let (mut h, _) = running_from_slot_a();
    let img = image(400, 11);
    h.deliver(&begin(0, img.len() as u32, crc32(&img)));
    h.deliver(&Frame::new(0, Cmd::Reset));
    assert_eq!(h.expected_seq(), 0);
    let mut seq = 0;
    send_image(&mut h, &mut seq, &img);
    h.deliver(&Frame::new(seq, Cmd::UpdateCommit));
    assert_eq!(h.data(), 0);
    assert_eq!(header_of(&h, Slot::B).generation.get(), 1);
}

#[test]
fn begin_restarts_the_session_and_erases_again() {
    let (mut h, _) = running_from_slot_a();
    let img = image(900, 12);
    h.deliver(&begin(0, img.len() as u32, crc32(&img)));
    h.deliver(&chunk(1, 0, &img[..100]));
    h.deliver(&begin(2, img.len() as u32, crc32(&img)));
    assert_eq!(h.data(), 0);
    assert_eq!(h.port.erased.len(), 2);
    assert!(slot_image(&h, Slot::B, 100).iter().all(|&b| b == 0xFF));
}

#[test]
fn begin_before_confirmation_rewrites_the_unconfirmed_slot() {
    let (mut h, running) = running_from_slot_a();
    let img = image(700, 15);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    assert_eq!(header_of(&h, Slot::B).generation.get(), 1);

    let next = image(800, 16);
    run_update(&mut h, &mut seq, &next);
    assert_eq!(header_of(&h, Slot::B).generation.get(), 2);
    assert_eq!(slot_image(&h, Slot::B, next.len()), &next[..]);
    assert_eq!(header_of(&h, Slot::A).generation.get(), 0);
    assert_eq!(slot_image(&h, Slot::A, running.len()), &running[..]);
}

#[test]
fn begin_after_confirmation_targets_the_older_slot() {
    let (mut h, _) = running_from_slot_a();
    let img = image(700, 15);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.reboot();
    h.deliver(&Frame::new(0, Cmd::UpdateConfirm));
    assert_eq!(h.data(), 0);

    let next = image(800, 16);
    let mut seq = 1;
    run_update(&mut h, &mut seq, &next);
    assert_eq!(header_of(&h, Slot::A).generation.get(), 2);
    assert_eq!(slot_image(&h, Slot::A, next.len()), &next[..]);
    assert_eq!(header_of(&h, Slot::B).generation.get(), 1);
    assert_eq!(slot_image(&h, Slot::B, img.len()), &img[..]);
}

fn confirm(h: &mut Harness, seq: u8) -> u8 {
    h.deliver(&Frame::new(seq, Cmd::UpdateConfirm));
    h.data()
}

#[test]
fn commit_writes_the_image_as_an_untried_trial() {
    let (mut h, _) = running_from_slot_a();
    let img = image(900, 20);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    let b = header_of(&h, Slot::B);
    assert!(b.is_trial());
    assert_eq!(b.attempts_used(), 0);
    assert!(b.is_boot_eligible());
    assert!(!header_of(&h, Slot::A).needs_confirmation());
}

#[test]
fn booting_a_trial_spends_its_single_attempt() {
    let (mut h, _) = running_from_slot_a();
    let img = image(900, 21);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);

    h.reboot();
    assert_eq!(h.cpu.booted_slot(), Some(Slot::B));
    let b = header_of(&h, Slot::B);
    assert_eq!(b.attempts_used(), 1);
    assert!(!b.is_boot_eligible());
    assert_eq!(slot_image(&h, Slot::B, img.len()), &img[..]);
}

#[test]
fn an_unconfirmed_trial_rolls_back_on_the_next_boot() {
    let (mut h, running) = running_from_slot_a();
    let img = image(900, 22);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);

    h.reboot();
    assert_eq!(h.cpu.booted_slot(), Some(Slot::B));
    h.reboot();
    assert_eq!(h.cpu.booted_slot(), Some(Slot::A));
    assert_eq!(header_of(&h, Slot::B).attempts_used(), 1);
    assert_eq!(slot_image(&h, Slot::A, running.len()), &running[..]);
    h.reboot();
    assert_eq!(h.cpu.booted_slot(), Some(Slot::A));
}

#[test]
fn a_confirmed_trial_keeps_booting() {
    let (mut h, _) = running_from_slot_a();
    let img = image(900, 23);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);

    h.reboot();
    assert_eq!(confirm(&mut h, 0), 0);
    let b = header_of(&h, Slot::B);
    assert!(!b.needs_confirmation());
    assert!(b.is_boot_eligible());
    assert_eq!(confirm(&mut h, 1), 0);

    for _ in 0..3 {
        h.reboot();
        assert_eq!(h.cpu.booted_slot(), Some(Slot::B));
    }
    assert_eq!(header_of(&h, Slot::B).attempts_used(), 1);
}

#[test]
fn confirming_a_normal_image_writes_nothing() {
    let (mut h, _) = running_from_slot_a();
    h.reboot();
    assert_eq!(h.cpu.booted_slot(), Some(Slot::A));
    let before = h.port.flash.clone();
    assert_eq!(confirm(&mut h, 0), 0);
    assert_eq!(h.port.flash, before);
    assert_eq!(header_of(&h, Slot::A).attempts_used(), 0);
}

#[test]
fn confirm_without_a_booted_image_is_rejected() {
    let (mut h, _) = running_from_slot_a();
    assert_eq!(h.cpu.booted_slot(), None);
    assert_eq!(confirm(&mut h, 0), Error::UpdateNothingToConfirm as u8);

    let mut blank = Harness::new();
    blank.reboot();
    assert_eq!(blank.cpu.booted_slot(), None);
    assert_eq!(confirm(&mut blank, 0), Error::UpdateNothingToConfirm as u8);
}

#[test]
fn begin_over_the_booted_trial_forgets_it_so_confirm_cannot_bless_the_next_image() {
    let (mut h, running) = running_from_slot_a();
    let img = image(900, 24);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.reboot();
    assert_eq!(h.cpu.booted_slot(), Some(Slot::B));

    let next = image(1100, 25);
    let mut seq = 0;
    h.deliver(&begin(seq, next.len() as u32, crc32(&next)));
    assert_eq!(h.data(), 0);
    seq += 1;
    assert_eq!(h.cpu.booted_slot(), None);
    assert_eq!(confirm(&mut h, seq), Error::UpdateNothingToConfirm as u8);
    seq += 1;
    send_image(&mut h, &mut seq, &next);
    h.deliver(&Frame::new(seq, Cmd::UpdateCommit));
    assert_eq!(h.data(), 0);

    let b = header_of(&h, Slot::B);
    assert!(b.is_trial());
    assert_eq!(b.attempts_used(), 0);
    assert_eq!(b.generation.get(), 2);
    assert_eq!(slot_image(&h, Slot::A, running.len()), &running[..]);
}

#[test]
fn confirm_refuses_when_the_booted_header_changed_underneath() {
    let (mut h, _) = running_from_slot_a();
    let img = image(900, 26);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.reboot();
    let base = Slot::B.base() as usize;
    h.port.flash[base + 4] ^= 0x01;
    assert_eq!(confirm(&mut h, 0), Error::UpdateNothingToConfirm as u8);
    assert!(header_of(&h, Slot::B).is_trial());
}

#[test]
fn a_spent_trial_still_boots_when_no_other_slot_is_valid() {
    let mut h = Harness::new();
    let img = image(900, 27);
    let mut header = ImageHeader::new_trial(4, img.len() as u32, crc32(&img));
    header.attempts = U32::new(0xFFFF_FFFE);
    let base = Slot::B.base() as usize;
    h.port.flash[base..base + HEADER_BYTES].copy_from_slice(header.as_bytes());
    let image_base = Slot::B.image_base() as usize;
    h.port.flash[image_base..image_base + img.len()].copy_from_slice(&img);

    h.reboot();
    assert_eq!(h.cpu.booted_slot(), Some(Slot::B));
    assert_eq!(header_of(&h, Slot::B).attempts, U32::new(0xFFFF_FFFE));
}

#[test]
fn an_interrupted_confirm_counts_as_unconfirmed() {
    let (mut h, _) = running_from_slot_a();
    let img = image(900, 28);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.reboot();
    let status = Slot::B.base() as usize + 16;
    h.port.flash[status + 3] = 0x00;
    h.reboot();
    assert_eq!(h.cpu.booted_slot(), Some(Slot::A));
}

#[test]
fn a_flash_failure_while_marking_does_not_stop_the_boot() {
    let (mut h, _) = running_from_slot_a();
    let img = image(900, 29);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.port.flash_fail = true;
    h.reboot();
    assert_eq!(h.cpu.booted_slot(), None);
    h.port.flash_fail = false;
    assert_eq!(header_of(&h, Slot::B).attempts_used(), 0);
}

#[test]
fn resending_an_identical_chunk_is_harmless() {
    let (mut h, _) = running_from_slot_a();
    let img = image(1500, 13);
    h.deliver(&begin(0, img.len() as u32, crc32(&img)));
    let mut seq = 1;
    send_image(&mut h, &mut seq, &img);
    h.deliver(&chunk(seq, 0, &img[..UPDATE_CHUNK_MAX_DATA_LEN]));
    assert_eq!(h.data(), 0);
    h.deliver(&Frame::new(seq + 1, Cmd::UpdateCommit));
    assert_eq!(h.data(), 0);
}

#[test]
fn commit_after_commit_is_rejected_without_touching_the_header() {
    let (mut h, _) = running_from_slot_a();
    let img = image(128, 14);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.deliver(&Frame::new(seq, Cmd::UpdateCommit));
    assert_eq!(h.data(), Error::UpdateNotStarted as u8);
    assert_eq!(header_of(&h, Slot::B).generation.get(), 1);
    assert_eq!(h.cpu.update.state(), State::Committed);
}

fn fpga_begin(seq: u8, length: u32, crc: u32) -> Frame {
    Frame::from_payload(
        seq,
        Cmd::FpgaUpdateBegin,
        &UpdateBeginPayload {
            length: U32::new(length),
            crc32: U32::new(crc),
        },
    )
}

#[test]
fn begin_is_rejected_while_an_activation_is_pending() {
    let (mut h, _) = running_from_slot_a();
    let img = image(300, 31);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.deliver(&Frame::new(seq, Cmd::UpdateActivate));
    assert_eq!(h.data(), 0);
    seq += 1;
    let erased_before = h.port.erased.len();
    h.deliver(&begin(seq, img.len() as u32, crc32(&img)));
    assert_eq!(h.data(), Error::UpdateActivating as u8);
    seq += 1;
    h.deliver(&fpga_begin(seq, 100, 0));
    assert_eq!(h.data(), Error::UpdateActivating as u8);
    assert_eq!(h.port.erased.len(), erased_before);
    assert!(h.port.fpga_flash_ops.is_empty());
    assert!(!h.cpu.fpga_update.is_locked());
    assert_eq!(h.cpu.update.state(), State::Committed);
    h.tick_1ms(u32::from(ACTIVATE_DELAY_MS));
    assert_eq!(h.port.reset_count, 1);
}

#[test]
fn a_chunk_with_no_data_writes_nothing() {
    let (mut h, _) = running_from_slot_a();
    let img = image(300, 32);
    h.deliver(&begin(0, img.len() as u32, crc32(&img)));
    h.port.flash_write_fail_after = Some(0);
    h.deliver(&chunk(1, 0, &[]));
    assert_eq!(h.data(), 0);
    h.deliver(&chunk(2, img.len() as u32, &[]));
    assert_eq!(h.data(), 0);
    h.deliver(&chunk(3, 0, &img[..1]));
    assert_eq!(h.data(), Error::UpdateFlash as u8);
}

#[test]
fn a_header_write_failure_fails_the_commit_and_leaves_the_slot_invalid() {
    let (mut h, _) = running_from_slot_a();
    let img = image(300, 33);
    h.deliver(&begin(0, img.len() as u32, crc32(&img)));
    let mut seq = 1;
    send_image(&mut h, &mut seq, &img);
    h.port.flash_write_fail_after = Some(0);
    h.deliver(&Frame::new(seq, Cmd::UpdateCommit));
    assert_eq!(h.data(), Error::UpdateFlash as u8);
    assert!(!header_of(&h, Slot::B).is_plausible());
    assert_ne!(h.cpu.update.state(), State::Committed);
}

#[test]
fn a_confirm_that_does_not_stick_is_reported() {
    let (mut h, _) = running_from_slot_a();
    let img = image(300, 34);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.reboot();
    assert_eq!(h.cpu.booted_slot(), Some(Slot::B));
    h.port.flash_write_silent = true;
    assert_eq!(confirm(&mut h, 0), Error::UpdateFlash as u8);
    assert!(header_of(&h, Slot::B).needs_confirmation());
    h.port.flash_write_silent = false;
    h.port.flash_write_fail_after = Some(0);
    assert_eq!(confirm(&mut h, 1), Error::UpdateFlash as u8);
    h.port.flash_write_fail_after = None;
    assert_eq!(confirm(&mut h, 2), 0);
    assert!(!header_of(&h, Slot::B).needs_confirmation());
}

#[test]
fn confirm_is_deferred_even_in_low_latency_mode() {
    let (mut h, _) = running_from_slot_a();
    let img = image(300, 35);
    let mut seq = 0;
    run_update(&mut h, &mut seq, &img);
    h.reboot();
    h.cpu.set_mode(Mode::LowLatency);
    h.deliver_no_drain(&Frame::new(0, Cmd::Nop));
    assert_eq!(h.ack(), 0);
    h.deliver_no_drain(&Frame::new(1, Cmd::UpdateConfirm));
    assert_eq!(h.ack(), 0);
    assert!(header_of(&h, Slot::B).needs_confirmation());
    assert!(h.process_one());
    assert_eq!(h.ack(), 1);
    assert_eq!(h.data(), 0);
    assert!(!header_of(&h, Slot::B).needs_confirmation());
    assert!(!h.process_one());
}
