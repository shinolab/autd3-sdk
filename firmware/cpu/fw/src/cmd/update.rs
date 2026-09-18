use core::cell::Cell;

use zerocopy::{FromBytes, IntoBytes};

pub use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
pub use autd3_cpu_wire::payload::{UpdateBeginPayload, UpdateChunkPayload};
use autd3_cpu_wire::update::{
    CRC32_INIT, IMAGE_HEADER_ATTEMPTS_OFFSET, IMAGE_HEADER_STATUS_OFFSET, IMAGE_MAX_ATTEMPTS,
    IMAGE_STATUS_CONFIRMED, SlotCandidate, next_attempts, select_boot_slot,
};
pub use autd3_cpu_wire::update::{
    FLASH_PAGE_BYTES, FLASH_SECTOR_BYTES, ImageHeader, SLOT_BYTES, SLOT_HEADER_BYTES, Slot,
    crc32_finish, crc32_update, is_plausible_length, select_slot,
};

use crate::app::Cpu;
use crate::port::{FlashError, Port};
use crate::proto::{Cmd, Error};

pub(crate) const ACTIVATE_DELAY_MS: u16 = 100;

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
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct BootImage {
    pub(crate) slot: Slot,
    generation: u32,
    crc32: u32,
}

pub(crate) struct UpdateSession {
    state: Cell<State>,
    reset_countdown: Cell<u16>,
    boot: Cell<Option<BootImage>>,
}

impl UpdateSession {
    pub(crate) const fn new() -> Self {
        Self {
            state: Cell::new(State::Idle),
            reset_countdown: Cell::new(0),
            boot: Cell::new(None),
        }
    }

    pub(crate) fn boot_slot(&self) -> Option<Slot> {
        self.boot.get().map(|b| b.slot)
    }

    pub(crate) fn init(&self) {
        self.state.set(State::Idle);
        self.reset_countdown.set(0);
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn state(&self) -> State {
        self.state.get()
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn reset_countdown(&self) -> u16 {
        self.reset_countdown.get()
    }

    pub(crate) fn is_activating(&self) -> bool {
        self.reset_countdown.get() != 0
    }
}

pub(crate) fn is_update_cmd(cmd: Cmd) -> bool {
    matches!(
        cmd,
        Cmd::UpdateBegin
            | Cmd::UpdateChunk
            | Cmd::UpdateCommit
            | Cmd::UpdateActivate
            | Cmd::UpdateConfirm
    )
}

impl From<FlashError> for Error {
    fn from(_: FlashError) -> Self {
        Self::UpdateFlash
    }
}

fn slot_range_ok(offset: u32, len: u32) -> bool {
    offset <= SLOT_BYTES && len <= SLOT_BYTES - offset
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
    let mut crc = CRC32_INIT;
    let mut buf = [0u8; READBACK_BYTES];
    let mut offset = 0u32;
    while offset < length {
        let n = (length - offset).min(READBACK_BYTES as u32) as usize;
        slot_read(port, slot, SLOT_HEADER_BYTES + offset, &mut buf[..n])?;
        crc = crc32_update(crc, &buf[..n]);
        offset += n as u32;
    }
    Ok(crc32_finish(crc))
}

fn read_header<P: Port>(port: &mut P, slot: Slot) -> Result<ImageHeader, Error> {
    let mut raw = [0u8; core::mem::size_of::<ImageHeader>()];
    slot_read(port, slot, 0, &mut raw)?;
    ImageHeader::read_from_bytes(&raw).map_err(|_| Error::UpdateFlash)
}

fn candidate<P: Port>(
    port: &mut P,
    slot: Slot,
) -> Result<Option<(SlotCandidate, ImageHeader)>, Error> {
    let header = read_header(port, slot)?;
    if !header.is_plausible() {
        return Ok(None);
    }
    if image_crc32(port, slot, header.length.get())? != header.crc32.get() {
        return Ok(None);
    }
    let candidate = SlotCandidate {
        generation: header.generation.get(),
        eligible: header.is_boot_eligible(),
    };
    Ok(Some((candidate, header)))
}

fn boot_selection<P: Port>(port: &mut P) -> Result<Option<(Slot, ImageHeader)>, Error> {
    let a = candidate(port, Slot::A)?;
    let b = candidate(port, Slot::B)?;
    let header_of = |slot| match slot {
        Slot::A => a.map(|(_, h)| h),
        Slot::B => b.map(|(_, h)| h),
    };
    Ok(select_boot_slot(a.map(|(c, _)| c), b.map(|(c, _)| c))
        .and_then(|(slot, _)| header_of(slot).map(|h| (slot, h))))
}

impl Cpu {
    pub(crate) fn update_begin<P: Port>(&self, port: &mut P, payload: &[u8]) -> Result<(), Error> {
        if self.update.is_activating() {
            return Err(Error::UpdateActivating);
        }
        self.update.state.set(State::Idle);
        let Ok((p, _)) = UpdateBeginPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let length = p.length.get();
        if !is_plausible_length(length) {
            return Err(Error::InvalidPayload);
        }
        let a = candidate(port, Slot::A)?;
        let b = candidate(port, Slot::B)?;
        let generation_of = |c: Option<(SlotCandidate, ImageHeader)>| c.map(|(c, _)| c.generation);
        let known_good = |c: Option<(SlotCandidate, ImageHeader)>| {
            c.filter(|(_, h)| !h.needs_confirmation())
                .map(|(c, _)| c.generation)
        };
        let Some((keep, _)) = select_slot(known_good(a), known_good(b))
            .or_else(|| select_boot_slot(a.map(|(c, _)| c), b.map(|(c, _)| c)))
        else {
            return Err(Error::UpdateFlash);
        };
        let slot = keep.other();
        let newest = generation_of(a)
            .into_iter()
            .chain(generation_of(b))
            .max()
            .unwrap_or(0);
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
        let Ok((p, rest)) = UpdateChunkPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let offset = p.offset.get();
        let data_len = p.data_len.get();
        if usize::from(data_len) > UPDATE_CHUNK_MAX_DATA_LEN
            || offset > length
            || u32::from(data_len) > length - offset
        {
            return Err(Error::InvalidPayload);
        }
        if data_len == 0 {
            return Ok(());
        }
        slot_write(
            port,
            slot,
            SLOT_HEADER_BYTES + offset,
            &rest[..usize::from(data_len)],
        )
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
            &IMAGE_STATUS_CONFIRMED.to_le_bytes(),
        )?;
        if read_header(port, boot.slot)?.needs_confirmation() {
            return Err(Error::UpdateFlash);
        }
        Ok(())
    }

    pub(crate) fn update_activate(&self) -> Result<(), Error> {
        if self.update.state.get() != State::Committed {
            return Err(Error::UpdateNotCommitted);
        }
        self.update.reset_countdown.set(ACTIVATE_DELAY_MS);
        Ok(())
    }

    pub(crate) fn update_tick<P: Port>(&self, port: &mut P) {
        let remaining = self.update.reset_countdown.get();
        if remaining == 0 {
            return;
        }
        let remaining = remaining - 1;
        self.update.reset_countdown.set(remaining);
        if remaining == 0 {
            port.reset();
        }
    }
}
