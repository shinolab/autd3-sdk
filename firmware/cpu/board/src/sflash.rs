use autd3_cpu_fw::port::FlashError;
use autd3_cpu_fw::update::{FLASH_PAGE_BYTES, FLASH_SECTOR_BYTES};

use crate::regs::{
    SPIBSC_CMNCR, SPIBSC_CMNSR, SPIBSC_DRCR, SPIBSC_SMADR, SPIBSC_SMCMR, SPIBSC_SMCR, SPIBSC_SMENR,
    SPIBSC_SMRDR0, SPIBSC_SMWDR0, read8, read32, write8, write32,
};

const XIP_BASE: usize = 0x3000_0000;

const CMNCR_MD_SPI: u32 = 1 << 31;
const CMNSR_TEND: u32 = 1;
const CMNSR_SSLF: u32 = 1 << 1;
const DRCR_SSLE: u32 = 1;
const DRCR_RBE: u32 = 1 << 8;
const DRCR_RCF: u32 = 1 << 9;
const DRCR_SSLN: u32 = 1 << 24;
const DRCR_BURST_HOLDS_SSL: u32 = DRCR_RBE | DRCR_SSLE;

const SMCR_SPIE: u32 = 1;
const SMCR_SPIWE: u32 = 1 << 1;
const SMCR_SPIRE: u32 = 1 << 2;
const SMCR_SSLKP: u32 = 1 << 8;

const SMENR_SPIDE_8_BITS: u32 = 0b1000;
const SMENR_ADE_24_BITS: u32 = 0b0111 << 8;
const SMENR_CDE: u32 = 1 << 14;
const SMCMR_CMD_SHIFT: u32 = 16;

const CMD_PAGE_PROGRAM: u32 = 0x02;
const CMD_READ_STATUS: u32 = 0x05;
const CMD_WRITE_ENABLE: u32 = 0x06;
const CMD_SECTOR_ERASE: u32 = 0x20;

const STATUS_WIP: u8 = 1;

const TRANSFER_END_MAX_POLLS: u32 = 1_000_000;
const IDLE_MAX_POLLS: u32 = 2_000_000;

fn wait_transfer_end() -> Result<(), FlashError> {
    for _ in 0..TRANSFER_END_MAX_POLLS {
        if read32(SPIBSC_CMNSR) & CMNSR_TEND != 0 {
            return Ok(());
        }
    }
    Err(FlashError)
}

fn transfer(enable: u32, control: u32) -> Result<(), FlashError> {
    write32(SPIBSC_SMENR, enable);
    write32(SPIBSC_SMCR, control);
    write32(SPIBSC_SMCR, control | SMCR_SPIE);
    wait_transfer_end()
}

fn command(cmd: u32) -> Result<(), FlashError> {
    write32(SPIBSC_SMCMR, cmd << SMCMR_CMD_SHIFT);
    transfer(SMENR_CDE, 0)
}

fn wait_idle() -> Result<(), FlashError> {
    for _ in 0..IDLE_MAX_POLLS {
        write32(SPIBSC_SMCMR, CMD_READ_STATUS << SMCMR_CMD_SHIFT);
        transfer(SMENR_CDE | SMENR_SPIDE_8_BITS, SMCR_SPIRE)?;
        if read8(SPIBSC_SMRDR0) & STATUS_WIP == 0 {
            return Ok(());
        }
    }
    Err(FlashError)
}

fn with_spi_mode<T>(f: impl FnOnce() -> Result<T, FlashError>) -> Result<T, FlashError> {
    fn release_read_burst() -> Result<(), FlashError> {
        let drcr = read32(SPIBSC_DRCR);
        if drcr & DRCR_BURST_HOLDS_SSL != DRCR_BURST_HOLDS_SSL {
            return Ok(());
        }
        write32(SPIBSC_DRCR, drcr | DRCR_SSLN);
        for _ in 0..TRANSFER_END_MAX_POLLS {
            if read32(SPIBSC_CMNSR) & CMNSR_SSLF == 0 {
                return Ok(());
            }
        }
        Err(FlashError)
    }

    release_read_burst()?;
    wait_transfer_end()?;
    let read_mode = read32(SPIBSC_CMNCR) & !CMNCR_MD_SPI;
    write32(SPIBSC_CMNCR, read_mode | CMNCR_MD_SPI);
    let result = f();
    let ended = wait_transfer_end();
    write32(SPIBSC_CMNCR, read_mode);
    write32(SPIBSC_DRCR, read32(SPIBSC_DRCR) | DRCR_RCF);
    let _ = read32(SPIBSC_DRCR);
    ended.and(result)
}

pub(crate) fn read(addr: u32, buf: &mut [u8]) -> Result<(), FlashError> {
    with_spi_mode(wait_idle)?;
    let base = XIP_BASE + addr as usize;
    if base.is_multiple_of(4) {
        let (words, tail) = buf.as_chunks_mut::<4>();
        let mut at = base;
        for word in words {
            *word = read32(at).to_le_bytes();
            at += 4;
        }
        for byte in tail {
            *byte = read8(at);
            at += 1;
        }
    } else {
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = read8(base + i);
        }
    }
    Ok(())
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn write(addr: u32, data: &[u8]) -> Result<(), FlashError> {
    fn program_page(addr: u32, data: &[u8]) -> Result<(), FlashError> {
        let Some((last, head)) = data.split_last() else {
            return Ok(());
        };
        wait_idle()?;
        command(CMD_WRITE_ENABLE)?;
        write32(SPIBSC_SMCMR, CMD_PAGE_PROGRAM << SMCMR_CMD_SHIFT);
        write32(SPIBSC_SMADR, addr);
        transfer(SMENR_CDE | SMENR_ADE_24_BITS, SMCR_SSLKP)?;
        for &byte in head {
            write8(SPIBSC_SMWDR0, byte);
            transfer(SMENR_SPIDE_8_BITS, SMCR_SPIWE | SMCR_SSLKP)?;
        }
        write8(SPIBSC_SMWDR0, *last);
        transfer(SMENR_SPIDE_8_BITS, SMCR_SPIWE)?;
        wait_idle()
    }

    with_spi_mode(|| {
        let mut at = addr;
        let mut rest = data;
        while !rest.is_empty() {
            let room = (FLASH_PAGE_BYTES - at % FLASH_PAGE_BYTES) as usize;
            let (page, tail) = rest.split_at(room.min(rest.len()));
            program_page(at, page)?;
            at += page.len() as u32;
            rest = tail;
        }
        Ok(())
    })
}

pub(crate) fn erase(addr: u32, len: u32) -> Result<(), FlashError> {
    fn erase_sector(addr: u32) -> Result<(), FlashError> {
        wait_idle()?;
        command(CMD_WRITE_ENABLE)?;
        write32(SPIBSC_SMCMR, CMD_SECTOR_ERASE << SMCMR_CMD_SHIFT);
        write32(SPIBSC_SMADR, addr);
        transfer(SMENR_CDE | SMENR_ADE_24_BITS, 0)?;
        wait_idle()
    }

    with_spi_mode(|| {
        for sector in (addr..addr + len).step_by(FLASH_SECTOR_BYTES as usize) {
            erase_sector(sector)?;
        }
        Ok(())
    })
}
