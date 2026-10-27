use zerocopy::FromBytes;
use zerocopy::big_endian::U32;

use autd3_cpu_wire::fpga_update::{
    FPGA_BARRIER_BASE, FPGA_GOLDEN_REGION_END, FPGA_IMAGE_CAPACITY, FpgaBootImage,
    is_plausible_fpga_length,
};

pub const SYNC_WORD: u32 = 0xAA99_5566;
const FPGA_BARRIER_TIMER: u32 = 0x4000_4000;

const BARRIER_WORDS: [u32; 14] = [
    0xFFFF_FFFF,
    0xFFFF_FFFF,
    0x0000_00BB,
    0x1122_0044,
    0xFFFF_FFFF,
    0xFFFF_FFFF,
    SYNC_WORD,
    0x2000_0000,
    0x2000_0000,
    0x3002_2001,
    FPGA_BARRIER_TIMER,
    0x2000_0000,
    0x2000_0000,
    0x2000_0000,
];
const FPGA_BARRIER_BYTES: usize = BARRIER_WORDS.len() * 4;

const _: () =
    assert!(FPGA_BARRIER_BASE as usize + FPGA_BARRIER_BYTES <= FPGA_GOLDEN_REGION_END as usize);

#[must_use]
pub const fn fpga_barrier_image() -> [u8; FPGA_BARRIER_BYTES] {
    let mut out = [0u8; FPGA_BARRIER_BYTES];
    let mut i = 0;
    while i < BARRIER_WORDS.len() {
        let b = BARRIER_WORDS[i].to_be_bytes();
        out[4 * i] = b[0];
        out[4 * i + 1] = b[1];
        out[4 * i + 2] = b[2];
        out[4 * i + 3] = b[3];
        i += 1;
    }
    out
}

const REG_CMD: u32 = 0x04;
const REG_AXSS: u32 = 0x0D;
const REG_WBSTAR: u32 = 0x10;
const REG_TIMER: u32 = 0x11;
const TIMER_CFG_MON: u32 = 1 << 30;
const CMD_IPROG: u32 = 0x0F;
const CMD_DESYNC: u32 = 0x0D;

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct BitstreamSummary {
    pub synced: bool,
    pub usr_access: Option<u32>,
    pub wbstar: Option<u32>,
    pub timer: Option<u32>,
    pub iprog: bool,
}

impl BitstreamSummary {
    #[must_use]
    pub fn boot_image(&self) -> FpgaBootImage {
        self.usr_access
            .map_or(FpgaBootImage::Unknown, FpgaBootImage::from_usr_access)
    }

    #[must_use]
    pub fn monitors_configuration(&self) -> bool {
        self.timer
            .is_some_and(|t| t & TIMER_CFG_MON != 0 && t & !(TIMER_CFG_MON | (1 << 31)) != 0)
    }
}

fn word_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let (word, _) = U32::read_from_prefix(bytes.get(offset..)?).ok()?;
    Some(word.get())
}

#[must_use]
pub fn summarize_bitstream(bytes: &[u8]) -> BitstreamSummary {
    fn find_sync(bytes: &[u8], from: usize) -> Option<usize> {
        let pattern = SYNC_WORD.to_be_bytes();
        bytes
            .get(from..)?
            .windows(4)
            .position(|w| w == pattern)
            .map(|p| from + p)
    }

    let mut summary = BitstreamSummary::default();
    let mut cursor = 0;
    while let Some(sync) = find_sync(bytes, cursor) {
        summary.synced = true;
        let mut offset = sync + 4;
        let mut last_reg = None;
        loop {
            let Some(header) = word_at(bytes, offset) else {
                return summary;
            };
            offset += 4;
            let (reg, count) = match header >> 29 {
                0b001 => {
                    let reg = (header >> 13) & 0x3FFF;
                    let write = (header >> 27) & 0b11 == 0b10;
                    last_reg = Some((reg, write));
                    (Some((reg, write)), (header & 0x7FF) as usize)
                }
                0b010 => (last_reg, (header & 0x07FF_FFFF) as usize),
                _ => (None, 0),
            };
            let count = match reg {
                Some((_, true)) => count,
                _ => 0,
            };
            if let Some((reg, true)) = reg
                && count == 1
                && let Some(value) = word_at(bytes, offset)
            {
                match reg {
                    REG_AXSS => summary.usr_access = Some(value),
                    REG_WBSTAR => summary.wbstar = Some(value),
                    REG_TIMER => summary.timer = Some(value),
                    REG_CMD if value == CMD_IPROG => summary.iprog = true,
                    REG_CMD if value == CMD_DESYNC => {
                        cursor = offset + 4;
                        break;
                    }
                    _ => {}
                }
            }
            offset = offset.saturating_add(count.saturating_mul(4));
        }
    }
    summary
}

#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FpgaImageError {
    #[error("FPGA update image must not be empty")]
    Empty,
    #[error(
        "FPGA update image is {length} bytes; the update slot holds at most {FPGA_IMAGE_CAPACITY} bytes"
    )]
    TooLarge { length: usize },
    #[error("no bitstream sync word (0xAA995566) found")]
    NoSyncWord,
    #[error("the bitstream is not an update-slot image (USR_ACCESS reads as {boot_image:?})")]
    NotAnUpdateImage { boot_image: FpgaBootImage },
    #[error("the bitstream reboots into another image (WBSTAR/IPROG found)")]
    Reboots,
}

pub fn validate_update_image(bytes: &[u8]) -> Result<BitstreamSummary, FpgaImageError> {
    if bytes.is_empty() {
        return Err(FpgaImageError::Empty);
    }
    if u32::try_from(bytes.len()).map_or(true, |len| !is_plausible_fpga_length(len)) {
        return Err(FpgaImageError::TooLarge {
            length: bytes.len(),
        });
    }
    let summary = summarize_bitstream(bytes);
    if !summary.synced {
        return Err(FpgaImageError::NoSyncWord);
    }
    if summary.iprog || summary.wbstar.is_some_and(|addr| addr != 0) {
        return Err(FpgaImageError::Reboots);
    }
    match summary.boot_image() {
        FpgaBootImage::Update => Ok(summary),
        boot_image => Err(FpgaImageError::NotAnUpdateImage { boot_image }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autd3_cpu_wire::fpga_update::{
        FPGA_IMAGE_BASE, FPGA_USR_ACCESS_GOLDEN, FPGA_USR_ACCESS_UPDATE,
    };

    fn stream(words: &[u32]) -> Vec<u8> {
        let mut out = std::vec![0xFF; 16];
        out.extend_from_slice(&[0x00, 0x00, 0x00, 0xBB, 0x11, 0x22, 0x00, 0x44]);
        for w in words {
            out.extend_from_slice(&w.to_be_bytes());
        }
        out
    }

    const NOOP: u32 = 0x2000_0000;
    const WRITE_AXSS: u32 = 0x3001_A001;
    const WRITE_WBSTAR: u32 = 0x3002_0001;
    const WRITE_CMD: u32 = 0x3000_8001;
    const WRITE_TIMER: u32 = 0x3002_2001;
    const WRITE_FDRI: u32 = 0x3000_4000;

    #[test]
    fn an_update_image_passes() {
        let bytes = stream(&[
            SYNC_WORD,
            NOOP,
            WRITE_WBSTAR,
            0,
            WRITE_CMD,
            0,
            WRITE_AXSS,
            FPGA_USR_ACCESS_UPDATE,
            WRITE_FDRI,
            0x5000_0003,
            WRITE_WBSTAR,
            WRITE_CMD,
            CMD_IPROG,
            NOOP,
            WRITE_CMD,
            CMD_DESYNC,
            0xFFFF_FFFF,
        ]);
        let summary = validate_update_image(&bytes).unwrap();
        assert_eq!(summary.usr_access, Some(FPGA_USR_ACCESS_UPDATE));
        assert_eq!(summary.wbstar, Some(0));
        assert!(!summary.iprog);
    }

    #[test]
    fn the_timer_write_arms_the_watchdog_only_with_the_monitor_bit() {
        let armed = summarize_bitstream(&stream(&[SYNC_WORD, WRITE_TIMER, 0x401D_0000]));
        assert_eq!(armed.timer, Some(0x401D_0000));
        assert!(armed.monitors_configuration());
        assert!(
            !summarize_bitstream(&stream(&[SYNC_WORD, WRITE_TIMER, 0])).monitors_configuration()
        );
        assert!(
            !summarize_bitstream(&stream(&[SYNC_WORD, WRITE_TIMER, 0x001D_0000]))
                .monitors_configuration()
        );
        assert!(
            !summarize_bitstream(&stream(&[SYNC_WORD, WRITE_TIMER, TIMER_CFG_MON]))
                .monitors_configuration()
        );
        assert!(!summarize_bitstream(&stream(&[SYNC_WORD, NOOP])).monitors_configuration());
    }

    #[test]
    fn a_golden_image_is_rejected() {
        let bytes = stream(&[
            SYNC_WORD,
            NOOP,
            WRITE_WBSTAR,
            FPGA_IMAGE_BASE,
            WRITE_CMD,
            CMD_IPROG,
            NOOP,
            WRITE_AXSS,
            FPGA_USR_ACCESS_GOLDEN,
        ]);
        let summary = summarize_bitstream(&bytes);
        assert_eq!(summary.wbstar, Some(FPGA_IMAGE_BASE));
        assert!(summary.iprog);
        assert_eq!(summary.boot_image(), FpgaBootImage::Golden);
        assert_eq!(validate_update_image(&bytes), Err(FpgaImageError::Reboots));
    }

    #[test]
    fn a_nonzero_wbstar_alone_is_rejected() {
        assert_eq!(
            validate_update_image(&stream(&[
                SYNC_WORD,
                WRITE_WBSTAR,
                FPGA_IMAGE_BASE,
                WRITE_AXSS,
                FPGA_USR_ACCESS_UPDATE
            ])),
            Err(FpgaImageError::Reboots)
        );
    }

    #[test]
    fn a_missing_or_foreign_usr_access_is_rejected() {
        assert_eq!(
            validate_update_image(&stream(&[SYNC_WORD, NOOP])),
            Err(FpgaImageError::NotAnUpdateImage {
                boot_image: FpgaBootImage::Unknown
            })
        );
        assert_eq!(
            validate_update_image(&stream(&[SYNC_WORD, WRITE_AXSS, FPGA_USR_ACCESS_GOLDEN])),
            Err(FpgaImageError::NotAnUpdateImage {
                boot_image: FpgaBootImage::Golden
            })
        );
    }

    #[test]
    fn the_barrier_syncs_and_arms_a_short_watchdog() {
        let barrier = summarize_bitstream(&fpga_barrier_image());
        assert!(barrier.synced);
        assert_eq!(barrier.timer, Some(FPGA_BARRIER_TIMER));
        assert!(barrier.monitors_configuration());
        assert!(!barrier.iprog);
        assert_eq!(barrier.wbstar, None);
        assert_eq!(barrier.boot_image(), FpgaBootImage::Unknown);
    }

    #[test]
    fn a_stream_without_sync_is_rejected() {
        assert_eq!(validate_update_image(&[]), Err(FpgaImageError::Empty));
        assert_eq!(
            validate_update_image(&[0xFF; 64]),
            Err(FpgaImageError::NoSyncWord)
        );
    }

    #[test]
    fn packets_after_a_desync_are_found_after_the_next_sync() {
        let bytes = stream(&[
            SYNC_WORD,
            WRITE_CMD,
            CMD_DESYNC,
            0xFFFF_FFFF,
            SYNC_WORD,
            WRITE_AXSS,
            FPGA_USR_ACCESS_UPDATE,
        ]);
        assert_eq!(
            summarize_bitstream(&bytes).boot_image(),
            FpgaBootImage::Update
        );
    }

    #[test]
    fn data_words_are_not_parsed_as_packets() {
        let bytes = stream(&[
            SYNC_WORD,
            WRITE_FDRI,
            0x5000_0002,
            WRITE_CMD,
            CMD_IPROG,
            WRITE_AXSS,
            FPGA_USR_ACCESS_UPDATE,
        ]);
        let summary = summarize_bitstream(&bytes);
        assert!(!summary.iprog);
        assert_eq!(summary.boot_image(), FpgaBootImage::Update);
    }
}
