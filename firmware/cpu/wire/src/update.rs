use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

pub const FLASH_BYTES: u32 = 0x40_0000;
pub const FLASH_SECTOR_BYTES: u32 = 0x1000;
pub const FLASH_PAGE_BYTES: u32 = 0x100;
pub const LOADER_REGION_END: u32 = 0x1_0000;
pub const SLOT_BYTES: u32 = 0x8_0000;
pub const SLOT_A_BASE: u32 = LOADER_REGION_END;
pub const SLOT_B_BASE: u32 = SLOT_A_BASE + SLOT_BYTES;
pub const SLOT_HEADER_BYTES: u32 = FLASH_PAGE_BYTES;
pub const SLOT_IMAGE_CAPACITY: u32 = SLOT_BYTES - SLOT_HEADER_BYTES;
pub const IMAGE_MAGIC: u32 = u32::from_le_bytes(*b"AUT2");
pub const IMAGE_VECTOR_BYTES: u32 = 0x40;
pub const IMAGE_VECTOR_LOAD_ADDR: u32 = 0x0000_0000;
pub const IMAGE_RESET_VECTOR_OFFSET: u32 = 0x20;
pub const IMAGE_APP_LOAD_ADDR: u32 = 0x0004_0000;
pub const ATCM_APP_REGION_END: u32 = 0x0007_0000;
pub const IMAGE_APP_CAPACITY: u32 = ATCM_APP_REGION_END - IMAGE_APP_LOAD_ADDR;

#[must_use]
pub const fn is_plausible_length(length: u32) -> bool {
    length > IMAGE_VECTOR_BYTES
        && length <= SLOT_IMAGE_CAPACITY
        && length - IMAGE_VECTOR_BYTES <= IMAGE_APP_CAPACITY
}

const _: () = assert!(IMAGE_VECTOR_BYTES + IMAGE_APP_CAPACITY <= SLOT_IMAGE_CAPACITY);
const _: () = assert!(IMAGE_RESET_VECTOR_OFFSET < IMAGE_VECTOR_BYTES);
const _: () = assert!(SLOT_A_BASE.is_multiple_of(FLASH_SECTOR_BYTES));
const _: () = assert!(SLOT_B_BASE.is_multiple_of(FLASH_SECTOR_BYTES));
const _: () = assert!(SLOT_B_BASE + SLOT_BYTES <= FLASH_BYTES);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    #[must_use]
    pub const fn base(self) -> u32 {
        match self {
            Self::A => SLOT_A_BASE,
            Self::B => SLOT_B_BASE,
        }
    }

    #[must_use]
    pub const fn image_base(self) -> u32 {
        self.base() + SLOT_HEADER_BYTES
    }

    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

pub const IMAGE_STATUS_NORMAL: u32 = 0xFFFF_FFFF;
pub const IMAGE_STATUS_TRIAL: u32 = 0x5A5A_5A5A;
pub const IMAGE_STATUS_CONFIRMED: u32 = 0x0000_0000;
pub const IMAGE_ATTEMPTS_UNTRIED: u32 = 0xFFFF_FFFF;
pub const IMAGE_MAX_ATTEMPTS: u32 = 1;

pub const IMAGE_HEADER_STATUS_OFFSET: u32 = 16;
pub const IMAGE_HEADER_ATTEMPTS_OFFSET: u32 = 20;

const _: () = assert!(IMAGE_STATUS_CONFIRMED & !IMAGE_STATUS_TRIAL == 0);
const _: () = assert!(IMAGE_MAX_ATTEMPTS >= 1 && IMAGE_MAX_ATTEMPTS <= u32::BITS);

#[must_use]
pub const fn next_attempts(attempts: u32) -> u32 {
    attempts << 1
}

#[derive(
    FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, PartialEq, Eq, Debug,
)]
#[repr(C)]
pub struct ImageHeader {
    pub magic: U32,
    pub generation: U32,
    pub length: U32,
    pub crc32: U32,
    pub status: U32,
    pub attempts: U32,
}

const _: () = assert!(core::mem::size_of::<ImageHeader>() == 24);
const _: () = assert!(core::mem::size_of::<ImageHeader>() <= SLOT_HEADER_BYTES as usize);
const _: () =
    assert!(core::mem::offset_of!(ImageHeader, status) == IMAGE_HEADER_STATUS_OFFSET as usize);
const _: () =
    assert!(core::mem::offset_of!(ImageHeader, attempts) == IMAGE_HEADER_ATTEMPTS_OFFSET as usize);

impl ImageHeader {
    #[must_use]
    pub fn new(generation: u32, length: u32, crc32: u32) -> Self {
        Self::with_status(generation, length, crc32, IMAGE_STATUS_NORMAL)
    }

    #[must_use]
    pub fn new_trial(generation: u32, length: u32, crc32: u32) -> Self {
        Self::with_status(generation, length, crc32, IMAGE_STATUS_TRIAL)
    }

    fn with_status(generation: u32, length: u32, crc32: u32, status: u32) -> Self {
        Self {
            magic: U32::new(IMAGE_MAGIC),
            generation: U32::new(generation),
            length: U32::new(length),
            crc32: U32::new(crc32),
            status: U32::new(status),
            attempts: U32::new(IMAGE_ATTEMPTS_UNTRIED),
        }
    }

    #[must_use]
    pub fn is_plausible(&self) -> bool {
        self.magic.get() == IMAGE_MAGIC && is_plausible_length(self.length.get())
    }

    #[must_use]
    pub fn is_trial(&self) -> bool {
        self.status.get() == IMAGE_STATUS_TRIAL
    }

    #[must_use]
    pub fn needs_confirmation(&self) -> bool {
        !matches!(
            self.status.get(),
            IMAGE_STATUS_NORMAL | IMAGE_STATUS_CONFIRMED
        )
    }

    #[must_use]
    pub fn attempts_used(&self) -> u32 {
        self.attempts.get().count_zeros()
    }

    #[must_use]
    pub fn is_boot_eligible(&self) -> bool {
        match self.status.get() {
            IMAGE_STATUS_NORMAL | IMAGE_STATUS_CONFIRMED => true,
            IMAGE_STATUS_TRIAL => self.attempts_used() < IMAGE_MAX_ATTEMPTS,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SlotCandidate {
    pub generation: u32,
    pub eligible: bool,
}

#[must_use]
pub fn select_slot(a: Option<u32>, b: Option<u32>) -> Option<(Slot, u32)> {
    match (a, b) {
        (Some(ga), Some(gb)) if gb > ga => Some((Slot::B, gb)),
        (Some(ga), _) => Some((Slot::A, ga)),
        (None, Some(gb)) => Some((Slot::B, gb)),
        (None, None) => None,
    }
}

#[must_use]
pub fn select_boot_slot(a: Option<SlotCandidate>, b: Option<SlotCandidate>) -> Option<(Slot, u32)> {
    let eligible = |c: Option<SlotCandidate>| c.filter(|c| c.eligible).map(|c| c.generation);
    let any = |c: Option<SlotCandidate>| c.map(|c| c.generation);
    select_slot(eligible(a), eligible(b)).or_else(|| select_slot(any(a), any(b)))
}

pub const CRC32_INIT: u32 = 0xFFFF_FFFF;

#[must_use]
pub const fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    let mut i = 0;
    while i < bytes.len() {
        crc ^= bytes[i] as u32;
        let mut bit = 0;
        while bit < 8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            bit += 1;
        }
        i += 1;
    }
    crc
}

#[must_use]
pub const fn crc32_finish(crc: u32) -> u32 {
    !crc
}

#[must_use]
pub const fn crc32(bytes: &[u8]) -> u32 {
    crc32_finish(crc32_update(CRC32_INIT, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_reference_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn crc32_streams_in_pieces() {
        let whole = crc32(b"hello world");
        let mut crc = crc32_update(CRC32_INIT, b"hello ");
        crc = crc32_update(crc, b"world");
        assert_eq!(crc32_finish(crc), whole);
    }

    #[test]
    fn slot_selection_prefers_the_newest_generation_and_a_on_ties() {
        assert_eq!(select_slot(None, None), None);
        assert_eq!(select_slot(Some(3), None), Some((Slot::A, 3)));
        assert_eq!(select_slot(None, Some(7)), Some((Slot::B, 7)));
        assert_eq!(select_slot(Some(3), Some(7)), Some((Slot::B, 7)));
        assert_eq!(select_slot(Some(9), Some(7)), Some((Slot::A, 9)));
        assert_eq!(select_slot(Some(5), Some(5)), Some((Slot::A, 5)));
    }

    #[test]
    fn header_plausibility() {
        let max = IMAGE_VECTOR_BYTES + IMAGE_APP_CAPACITY;
        assert!(ImageHeader::new(0, IMAGE_VECTOR_BYTES + 1, 0).is_plausible());
        assert!(ImageHeader::new(0, max, 0).is_plausible());
        assert!(!ImageHeader::new(0, 0, 0).is_plausible());
        assert!(!ImageHeader::new(0, IMAGE_VECTOR_BYTES, 0).is_plausible());
        assert!(!ImageHeader::new(0, max + 1, 0).is_plausible());
        assert!(!ImageHeader::new(0, SLOT_IMAGE_CAPACITY, 0).is_plausible());
        assert!(!ImageHeader::new(0, SLOT_IMAGE_CAPACITY + 1, 0).is_plausible());
        let mut blank = ImageHeader::new(0, IMAGE_VECTOR_BYTES + 1, 0);
        blank.magic = U32::new(0xFFFF_FFFF);
        assert!(!blank.is_plausible());
    }

    #[test]
    fn the_image_magic_spells_the_format() {
        assert_eq!(IMAGE_MAGIC.to_le_bytes(), *b"AUT2");
    }

    #[test]
    fn trial_state_machine_only_clears_bits() {
        let normal = ImageHeader::new(1, 100, 0);
        assert!(!normal.needs_confirmation());
        assert!(normal.is_boot_eligible());

        let mut trial = ImageHeader::new_trial(2, 100, 0);
        assert!(trial.is_trial());
        assert!(trial.needs_confirmation());
        assert_eq!(trial.attempts_used(), 0);
        assert!(trial.is_boot_eligible());
        assert_eq!(IMAGE_STATUS_TRIAL & normal.status.get(), IMAGE_STATUS_TRIAL);

        let before = trial.attempts.get();
        trial.attempts = U32::new(next_attempts(before));
        assert_eq!(trial.attempts.get() & !before, 0);
        assert_eq!(trial.attempts_used(), 1);
        assert!(!trial.is_boot_eligible());

        trial.status = U32::new(IMAGE_STATUS_CONFIRMED);
        assert!(!trial.needs_confirmation());
        assert!(trial.is_boot_eligible());
    }

    #[test]
    fn an_interrupted_status_write_is_not_eligible() {
        let mut h = ImageHeader::new_trial(2, 100, 0);
        h.status = U32::new(IMAGE_STATUS_TRIAL & 0x00FF_FFFF);
        assert!(h.needs_confirmation());
        assert!(!h.is_trial());
        assert!(!h.is_boot_eligible());
    }

    #[test]
    fn untried_trials_boot_and_tried_ones_fall_back() {
        let ok = |generation| {
            Some(SlotCandidate {
                generation,
                eligible: true,
            })
        };
        let spent = |generation| {
            Some(SlotCandidate {
                generation,
                eligible: false,
            })
        };
        assert_eq!(select_boot_slot(ok(1), ok(2)), Some((Slot::B, 2)));
        assert_eq!(select_boot_slot(ok(1), spent(2)), Some((Slot::A, 1)));
        assert_eq!(select_boot_slot(spent(3), ok(2)), Some((Slot::B, 2)));
        assert_eq!(select_boot_slot(spent(1), spent(2)), Some((Slot::B, 2)));
        assert_eq!(select_boot_slot(None, spent(2)), Some((Slot::B, 2)));
        assert_eq!(select_boot_slot(None, None), None);
    }

    #[test]
    fn slots_do_not_overlap_the_loader() {
        assert!(Slot::A.base() >= LOADER_REGION_END);
        assert_eq!(Slot::A.other(), Slot::B);
        assert_eq!(Slot::B.image_base(), SLOT_B_BASE + SLOT_HEADER_BYTES);
    }
}
