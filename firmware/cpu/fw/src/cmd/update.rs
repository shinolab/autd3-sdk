use zerocopy::{FromBytes, IntoBytes};

pub use autd3_cpu_wire::payload::{UpdateBeginPayload, UpdateChunkPayload};
use autd3_cpu_wire::update::{
    CRC32, IMAGE_HEADER_ATTEMPTS_OFFSET, IMAGE_HEADER_STATUS_OFFSET, IMAGE_MAX_ATTEMPTS,
    ImageStatus, next_attempts, select_boot_slot,
};
pub use autd3_cpu_wire::update::{
    FLASH_PAGE_BYTES, FLASH_SECTOR_BYTES, ImageHeader, SLOT_BYTES, SLOT_HEADER_BYTES, Slot,
    is_plausible_length, select_slot,
};

use crate::app::Cpu;
use crate::ctx::MainCell;
use crate::port::{FlashError, Port};
use crate::proto::Error;

const READBACK_BYTES: usize = FLASH_PAGE_BYTES as usize;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum State {
    Idle,
    Receiving {
        slot: Slot,
        length: u32,
        crc32: u32,
        generation: u32,
    },
    Committed,
    Activating {
        remaining_ms: u16,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct BootImage {
    pub(crate) slot: Slot,
    generation: u32,
    crc32: u32,
}

pub(crate) struct UpdateSession {
    state: MainCell<State>,
    boot: MainCell<Option<BootImage>>,
}

impl UpdateSession {
    pub(crate) const fn new() -> Self {
        Self {
            state: MainCell::new(State::Idle),
            boot: MainCell::new(None),
        }
    }

    pub(crate) fn boot_slot(&self) -> Option<Slot> {
        self.boot.get().map(|b| b.slot)
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn state(&self) -> State {
        self.state.get()
    }

    pub(crate) fn is_activating(&self) -> bool {
        matches!(self.state.get(), State::Activating { .. })
    }
}

impl From<FlashError> for Error {
    fn from(_: FlashError) -> Self {
        Self::UpdateFlash
    }
}

fn slot_range_ok(offset: u32, len: u32) -> bool {
    offset.saturating_add(len) <= SLOT_BYTES
}

fn slot_read<P: Port>(port: &mut P, slot: Slot, offset: u32, buf: &mut [u8]) -> Result<(), Error> {
    if !slot_range_ok(offset, buf.len() as u32) {
        return Err(Error::InvalidPayload);
    }
    port.flash_read(slot.base() + offset, buf)?;
    Ok(())
}

fn slot_write<P: Port>(port: &mut P, slot: Slot, offset: u32, data: &[u8]) -> Result<(), Error> {
    if !slot_range_ok(offset, data.len() as u32) {
        return Err(Error::InvalidPayload);
    }
    port.flash_write(slot.base() + offset, data)?;
    Ok(())
}

fn slot_erase<P: Port>(port: &mut P, slot: Slot, len: u32) -> Result<(), Error> {
    let len = len.div_ceil(FLASH_SECTOR_BYTES) * FLASH_SECTOR_BYTES;
    if !slot_range_ok(0, len) {
        return Err(Error::InvalidPayload);
    }
    port.flash_erase(slot.base(), len)?;
    Ok(())
}

fn image_crc32<P: Port>(port: &mut P, slot: Slot, length: u32) -> Result<u32, Error> {
    let mut digest = CRC32.digest();
    let mut buf = [0u8; READBACK_BYTES];
    let mut offset = 0u32;
    while offset < length {
        let n = (length - offset).min(READBACK_BYTES as u32) as usize;
        slot_read(port, slot, SLOT_HEADER_BYTES + offset, &mut buf[..n])?;
        digest.update(&buf[..n]);
        offset += n as u32;
    }
    Ok(digest.finalize())
}

fn read_header<P: Port>(port: &mut P, slot: Slot) -> Result<ImageHeader, Error> {
    let mut raw = [0u8; core::mem::size_of::<ImageHeader>()];
    slot_read(port, slot, 0, &mut raw)?;
    ImageHeader::read_from_bytes(&raw).map_err(|_| Error::UpdateFlash)
}

fn valid_header<P: Port>(port: &mut P, slot: Slot) -> Result<Option<ImageHeader>, Error> {
    let header = read_header(port, slot)?;
    if !header.is_plausible() {
        return Ok(None);
    }
    if image_crc32(port, slot, header.length.get())? != header.crc32.get() {
        return Ok(None);
    }
    Ok(Some(header))
}

fn boot_selection<P: Port>(port: &mut P) -> Result<Option<(Slot, ImageHeader)>, Error> {
    let a = valid_header(port, Slot::A)?;
    let b = valid_header(port, Slot::B)?;
    Ok(select_boot_slot(a.as_ref(), b.as_ref()).and_then(|slot| {
        let header = match slot {
            Slot::A => a,
            Slot::B => b,
        };
        header.map(|h| (slot, h))
    }))
}

impl Cpu {
    pub(crate) fn update_begin<P: Port>(&self, port: &mut P, payload: &[u8]) -> Result<(), Error> {
        if self.update.is_activating() {
            return Err(Error::UpdateActivating);
        }
        self.update.state.set(State::Idle);
        let p = UpdateBeginPayload::parse(payload)?;
        let length = p.length.get();
        if !is_plausible_length(length) {
            return Err(Error::InvalidPayload);
        }
        let a = valid_header(port, Slot::A)?;
        let b = valid_header(port, Slot::B)?;
        let generation_of = |h: Option<ImageHeader>| h.map(|h| h.generation.get());
        let known_good =
            |h: Option<ImageHeader>| generation_of(h.filter(|h| !h.needs_confirmation()));
        let Some(keep) = select_slot(known_good(a), known_good(b))
            .or_else(|| select_boot_slot(a.as_ref(), b.as_ref()))
        else {
            return Err(Error::UpdateFlash);
        };
        let slot = keep.other();
        let newest = generation_of(a).max(generation_of(b)).unwrap_or(0);
        let generation = newest.wrapping_add(1);
        if self.update.boot_slot() == Some(slot) {
            self.update.boot.set(None);
        }
        slot_erase(port, slot, SLOT_HEADER_BYTES + length)?;
        self.update.state.set(State::Receiving {
            slot,
            length,
            crc32: p.crc32.get(),
            generation,
        });
        Ok(())
    }

    pub(crate) fn update_chunk<P: Port>(&self, port: &mut P, payload: &[u8]) -> Result<(), Error> {
        let State::Receiving { slot, length, .. } = self.update.state.get() else {
            return Err(Error::UpdateNotStarted);
        };
        let (chunk, data) = UpdateChunkPayload::parse(payload)?;
        if !chunk.fits_in(data, length) {
            return Err(Error::InvalidPayload);
        }
        if data.is_empty() {
            return Ok(());
        }
        slot_write(port, slot, SLOT_HEADER_BYTES + chunk.offset.get(), data)
    }

    pub(crate) fn update_commit<P: Port>(&self, port: &mut P) -> Result<(), Error> {
        let State::Receiving {
            slot,
            length,
            crc32,
            generation,
        } = self.update.state.get()
        else {
            return Err(Error::UpdateNotStarted);
        };
        self.update.state.set(State::Idle);
        if image_crc32(port, slot, length)? != crc32 {
            return Err(Error::UpdateImageInvalid);
        }
        let header = ImageHeader::new_trial(generation, length, crc32);
        slot_write(port, slot, 0, header.as_bytes())?;
        self.update.state.set(State::Committed);
        Ok(())
    }

    pub(crate) fn record_boot_attempt<P: Port>(&self, port: &mut P) -> Result<(), Error> {
        self.update.boot.set(None);
        let Some((slot, header)) = boot_selection(port)? else {
            return Ok(());
        };
        self.update.boot.set(Some(BootImage {
            slot,
            generation: header.generation.get(),
            crc32: header.crc32.get(),
        }));
        if header.is_trial() && header.attempts_used() < IMAGE_MAX_ATTEMPTS {
            let attempts = next_attempts(header.attempts.get());
            slot_write(
                port,
                slot,
                IMAGE_HEADER_ATTEMPTS_OFFSET,
                &attempts.to_le_bytes(),
            )?;
        }
        Ok(())
    }

    pub(crate) fn update_confirm<P: Port>(&self, port: &mut P) -> Result<(), Error> {
        let Some(boot) = self.update.boot.get() else {
            return Err(Error::UpdateNothingToConfirm);
        };
        let header = read_header(port, boot.slot)?;
        if !header.is_plausible()
            || header.generation.get() != boot.generation
            || header.crc32.get() != boot.crc32
        {
            return Err(Error::UpdateNothingToConfirm);
        }
        if !header.needs_confirmation() {
            return Ok(());
        }
        slot_write(
            port,
            boot.slot,
            IMAGE_HEADER_STATUS_OFFSET,
            &ImageStatus::Confirmed.as_u32().to_le_bytes(),
        )?;
        if read_header(port, boot.slot)?.needs_confirmation() {
            return Err(Error::UpdateFlash);
        }
        Ok(())
    }

    pub(crate) fn update_activate(&self) -> Result<(), Error> {
        if !matches!(
            self.update.state.get(),
            State::Committed | State::Activating { .. }
        ) {
            return Err(Error::UpdateNotCommitted);
        }
        self.update.state.set(State::Activating {
            remaining_ms: self.config().update_activate_delay.as_millis() as u16,
        });
        Ok(())
    }

    pub(crate) fn update_tick<P: Port>(&self, port: &mut P) {
        let State::Activating { remaining_ms } = self.update.state.get() else {
            return;
        };
        let remaining_ms = remaining_ms - 1;
        if remaining_ms == 0 {
            self.update.state.set(State::Committed);
            port.reset();
        } else {
            self.update.state.set(State::Activating { remaining_ms });
        }
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec::Vec;

    use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
    use autd3_cpu_wire::update::{
        FLASH_SECTOR_BYTES, IMAGE_APP_CAPACITY, IMAGE_VECTOR_BYTES, ImageHeader, LOADER_REGION_END,
        SLOT_HEADER_BYTES, SLOT_IMAGE_CAPACITY, Slot, crc32,
    };
    use zerocopy::little_endian::U32;
    use zerocopy::{FromBytes, IntoBytes};

    use super::{State, UpdateBeginPayload, UpdateChunkPayload};

    const ACTIVATE_DELAY_MS: u16 =
        autd3_cpu_wire::cpu_params::UPDATE_ACTIVATE_DELAY.as_millis() as u16;
    use crate::proto::{Cmd, Error};
    use crate::test_utils::mock::{Frame, Harness};

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
            assert_eq!(h.status(), Error::None, "chunk {i}");
            *seq = seq.wrapping_add(1);
        }
    }

    fn run_update(h: &mut Harness, seq: &mut u8, img: &[u8]) {
        h.deliver(&begin(*seq, img.len() as u32, crc32(img)));
        assert_eq!(h.status(), Error::None);
        *seq = seq.wrapping_add(1);
        send_image(h, seq, img);
        h.deliver(&Frame::new(*seq, Cmd::UpdateCommit));
        assert_eq!(h.status(), Error::None);
        *seq = seq.wrapping_add(1);
    }

    fn header_of(h: &Harness, slot: Slot) -> ImageHeader {
        let base = slot.base() as usize;
        ImageHeader::read_from_bytes(&h.port.flash[base..][..size_of::<ImageHeader>()]).unwrap()
    }

    fn slot_image(h: &Harness, slot: Slot, len: usize) -> &[u8] {
        let base = slot.image_base() as usize;
        &h.port.flash[base..base + len]
    }

    fn stamp(h: &mut Harness, slot: Slot, generation: u32, img: &[u8]) {
        let header = ImageHeader::new(generation, img.len() as u32, crc32(img));
        let base = slot.base() as usize;
        h.port.flash[base..][..size_of::<ImageHeader>()].copy_from_slice(header.as_bytes());
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
        assert_eq!(h.status(), Error::UpdateFlash);
        assert_eq!(h.cpu.update.state(), State::Idle);
        assert_eq!(h.port.erased, []);
        assert!(h.port.flash.iter().all(|&b| b == 0xFF));

        h.deliver(&chunk(1, 0, &[1, 2, 3]));
        assert_eq!(h.status(), Error::UpdateNotStarted);
    }

    #[test]
    fn begin_refuses_when_the_only_header_has_a_wrong_crc() {
        let (mut h, _) = running_from_slot_a();
        let corrupt = Slot::A.image_base() as usize + 3;
        h.port.flash[corrupt] ^= 0x10;
        h.deliver(&begin(0, 5000, 0));
        assert_eq!(h.status(), Error::UpdateFlash);
        assert_eq!(h.port.erased, []);
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
        assert_eq!(h.status(), Error::UpdateImageInvalid);
        assert!(!header_of(&h, Slot::B).is_plausible());
        assert_eq!(header_of(&h, Slot::A).generation.get(), 0);
        assert_eq!(h.cpu.update.state(), State::Idle);

        h.deliver(&chunk(seq + 1, 0, &img[..8]));
        assert_eq!(h.status(), Error::UpdateNotStarted);
        h.deliver(&Frame::new(seq + 2, Cmd::UpdateCommit));
        assert_eq!(h.status(), Error::UpdateNotStarted);
        h.deliver(&Frame::new(seq + 3, Cmd::UpdateActivate));
        assert_eq!(h.status(), Error::UpdateNotCommitted);
    }

    #[test]
    fn chunk_and_commit_without_begin_are_rejected() {
        let mut h = Harness::new();
        h.deliver(&chunk(0, 0, &[1, 2, 3]));
        assert_eq!(h.status(), Error::UpdateNotStarted);
        h.deliver(&Frame::new(1, Cmd::UpdateCommit));
        assert_eq!(h.status(), Error::UpdateNotStarted);
        assert_eq!(h.port.erased, []);
        assert!(h.port.flash.iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn chunk_out_of_range_is_invalid_payload() {
        let (mut h, _) = running_from_slot_a();
        let img = image(1000, 8);
        h.deliver(&begin(0, img.len() as u32, crc32(&img)));
        h.deliver(&chunk(1, 996, &img[..8]));
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&chunk(2, 1001, &[]));
        assert_eq!(h.status(), Error::InvalidPayload);
        let oversized = Frame::from_parts(
            3,
            Cmd::UpdateChunk,
            &UpdateChunkPayload {
                offset: U32::new(0),
            },
            &[0; UPDATE_CHUNK_MAX_DATA_LEN + 1],
        );
        h.deliver(&oversized);
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&chunk(4, 1000, &[]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.cpu.update.state(),
            State::Receiving {
                slot: Slot::B,
                length: 1000,
                crc32: crc32(&img),
                generation: 1
            }
        );
        h.deliver(&chunk(5, u32::MAX - 1, &img[..3]));
        assert_eq!(h.status(), Error::InvalidPayload);
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
            assert_eq!(h.status(), Error::InvalidPayload, "length {length}");
        }
        assert_eq!(h.cpu.update.state(), State::Idle);
        assert_eq!(h.port.erased, []);
        h.deliver(&begin(5, max, 0));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.erased, [(Slot::B.base(), erased_len(max))]);
    }

    #[test]
    fn activate_waits_for_the_configured_delay() {
        use autd3_cpu_wire::config::CpuConfig;

        use crate::test_utils::builders::set_cpu_config;

        let (mut h, _) = running_from_slot_a();
        let img = image(300, 9);
        let mut seq = 0;
        h.deliver(&set_cpu_config(
            seq,
            &CpuConfig {
                update_activate_delay: core::time::Duration::from_millis(7),
                ..CpuConfig::default()
            },
        ));
        assert_eq!(h.status(), Error::None);
        seq += 1;
        h.deliver(&begin(seq, img.len() as u32, crc32(&img)));
        seq += 1;
        send_image(&mut h, &mut seq, &img);
        h.deliver(&Frame::new(seq, Cmd::UpdateCommit));
        seq += 1;
        h.deliver(&Frame::new(seq, Cmd::UpdateActivate));
        assert_eq!(h.status(), Error::None);

        h.tick_1ms(6);
        assert_eq!(h.port.reset_count, 0);
        h.tick_1ms(1);
        assert_eq!(h.port.reset_count, 1);
    }

    #[test]
    fn activate_requires_a_commit_and_resets_after_the_delay() {
        let (mut h, _) = running_from_slot_a();
        let img = image(300, 9);
        let mut seq = 0;
        h.deliver(&begin(seq, img.len() as u32, crc32(&img)));
        seq += 1;
        h.deliver(&Frame::new(seq, Cmd::UpdateActivate));
        assert_eq!(h.status(), Error::UpdateNotCommitted);
        seq += 1;
        send_image(&mut h, &mut seq, &img);
        h.deliver(&Frame::new(seq, Cmd::UpdateCommit));
        assert_eq!(h.status(), Error::None);
        seq += 1;
        h.deliver(&Frame::new(seq, Cmd::UpdateActivate));
        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.cpu.update.state(),
            State::Activating {
                remaining_ms: ACTIVATE_DELAY_MS
            }
        );

        h.tick_1ms(u32::from(ACTIVATE_DELAY_MS) - 1);
        assert_eq!(h.port.reset_count, 0);
        seq += 1;
        h.deliver(&Frame::new(seq, Cmd::UpdateActivate));
        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.cpu.update.state(),
            State::Activating {
                remaining_ms: ACTIVATE_DELAY_MS
            }
        );
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
        assert_eq!(h.status(), Error::UpdateFlash);
        assert_eq!(h.cpu.update.state(), State::Idle);
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
        assert_eq!(h.status(), Error::None);
        assert_eq!(header_of(&h, Slot::B).generation.get(), 1);
    }

    #[test]
    fn begin_restarts_the_session_and_erases_again() {
        let (mut h, _) = running_from_slot_a();
        let img = image(900, 12);
        h.deliver(&begin(0, img.len() as u32, crc32(&img)));
        h.deliver(&chunk(1, 0, &img[..100]));
        h.deliver(&begin(2, img.len() as u32, crc32(&img)));
        assert_eq!(h.status(), Error::None);
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
        assert_eq!(h.status(), Error::None);

        let next = image(800, 16);
        let mut seq = 1;
        run_update(&mut h, &mut seq, &next);
        assert_eq!(header_of(&h, Slot::A).generation.get(), 2);
        assert_eq!(slot_image(&h, Slot::A, next.len()), &next[..]);
        assert_eq!(header_of(&h, Slot::B).generation.get(), 1);
        assert_eq!(slot_image(&h, Slot::B, img.len()), &img[..]);
    }

    fn confirm(h: &mut Harness, seq: u8) -> Error {
        h.deliver(&Frame::new(seq, Cmd::UpdateConfirm));
        h.status()
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
        assert_eq!(confirm(&mut h, 0), Error::None);
        let b = header_of(&h, Slot::B);
        assert!(!b.needs_confirmation());
        assert!(b.is_boot_eligible());
        assert_eq!(confirm(&mut h, 1), Error::None);

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
        assert_eq!(confirm(&mut h, 0), Error::None);
        assert_eq!(h.port.flash, before);
        assert_eq!(header_of(&h, Slot::A).attempts_used(), 0);
    }

    #[test]
    fn confirm_without_a_booted_image_is_rejected() {
        let (mut h, _) = running_from_slot_a();
        assert_eq!(h.cpu.booted_slot(), None);
        assert_eq!(confirm(&mut h, 0), Error::UpdateNothingToConfirm);

        let mut blank = Harness::new();
        blank.reboot();
        assert_eq!(blank.cpu.booted_slot(), None);
        assert_eq!(confirm(&mut blank, 0), Error::UpdateNothingToConfirm);
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
        assert_eq!(h.status(), Error::None);
        seq += 1;
        assert_eq!(h.cpu.booted_slot(), None);
        assert_eq!(confirm(&mut h, seq), Error::UpdateNothingToConfirm);
        seq += 1;
        send_image(&mut h, &mut seq, &next);
        h.deliver(&Frame::new(seq, Cmd::UpdateCommit));
        assert_eq!(h.status(), Error::None);

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
        assert_eq!(confirm(&mut h, 0), Error::UpdateNothingToConfirm);
        assert!(header_of(&h, Slot::B).is_trial());
    }

    #[test]
    fn a_spent_trial_still_boots_when_no_other_slot_is_valid() {
        let mut h = Harness::new();
        let img = image(900, 27);
        let mut header = ImageHeader::new_trial(4, img.len() as u32, crc32(&img));
        header.attempts = U32::new(0xFFFF_FFFE);
        let base = Slot::B.base() as usize;
        h.port.flash[base..][..size_of::<ImageHeader>()].copy_from_slice(header.as_bytes());
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
        assert_eq!(h.status(), Error::None);
        h.deliver(&Frame::new(seq + 1, Cmd::UpdateCommit));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn commit_after_commit_is_rejected_without_touching_the_header() {
        let (mut h, _) = running_from_slot_a();
        let img = image(128, 14);
        let mut seq = 0;
        run_update(&mut h, &mut seq, &img);
        h.deliver(&Frame::new(seq, Cmd::UpdateCommit));
        assert_eq!(h.status(), Error::UpdateNotStarted);
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
        assert_eq!(h.status(), Error::None);
        seq += 1;
        let erased_before = h.port.erased.len();
        h.deliver(&begin(seq, img.len() as u32, crc32(&img)));
        assert_eq!(h.status(), Error::UpdateActivating);
        seq += 1;
        h.deliver(&fpga_begin(seq, 100, 0));
        assert_eq!(h.status(), Error::UpdateActivating);
        assert_eq!(h.port.erased.len(), erased_before);
        assert_eq!(h.port.fpga_flash_ops, []);
        assert!(!h.cpu.fpga_update.is_locked());
        assert!(h.cpu.update.is_activating());
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
        assert_eq!(h.status(), Error::None);
        h.deliver(&chunk(2, img.len() as u32, &[]));
        assert_eq!(h.status(), Error::None);
        h.deliver(&chunk(3, 0, &img[..1]));
        assert_eq!(h.status(), Error::UpdateFlash);
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
        assert_eq!(h.status(), Error::UpdateFlash);
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
        assert_eq!(confirm(&mut h, 0), Error::UpdateFlash);
        assert!(header_of(&h, Slot::B).needs_confirmation());
        h.port.flash_write_silent = false;
        h.port.flash_write_fail_after = Some(0);
        assert_eq!(confirm(&mut h, 1), Error::UpdateFlash);
        h.port.flash_write_fail_after = None;
        assert_eq!(confirm(&mut h, 2), Error::None);
        assert!(!header_of(&h, Slot::B).needs_confirmation());
    }
}
