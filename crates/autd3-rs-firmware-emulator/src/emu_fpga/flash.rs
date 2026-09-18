use autd3_cpu_fw::fpga_update::{
    FPGA_FLASH_BYTES, FPGA_GOLDEN_REGION_END, FPGA_IMAGE_BASE, FPGA_SECTOR_BYTES,
    FPGA_USR_ACCESS_GOLDEN, FPGA_USR_ACCESS_UPDATE, validate_update_image,
};
use autd3_cpu_fw::update::crc32;

use crate::fw;

const BUF_WORDS: usize = fw::FLASH_BUF_BYTES / 2;
const JEDEC_ID: u32 = 0x0020_BA18;

pub(crate) struct FlashEmulator {
    reg: [u16; 16],
    buf: Box<[u16; BUF_WORDS]>,
    flash: Vec<u8>,
    usr_access: u32,
    reboot_requested: bool,
    reboots_to_ignore: u32,
}

impl FlashEmulator {
    pub(crate) fn new() -> Self {
        Self {
            reg: [0; 16],
            buf: Box::new([0; BUF_WORDS]),
            flash: Vec::new(),
            usr_access: FPGA_USR_ACCESS_UPDATE,
            reboot_requested: false,
            reboots_to_ignore: 0,
        }
    }

    pub(crate) fn flash(&self) -> &[u8] {
        &self.flash
    }

    pub(crate) fn flash_mut(&mut self) -> &mut Vec<u8> {
        if self.flash.is_empty() {
            self.flash = vec![0xFF; FPGA_FLASH_BYTES as usize];
        }
        &mut self.flash
    }

    pub(crate) fn usr_access(&self) -> u32 {
        self.usr_access
    }

    pub(crate) fn ignore_reboots(&mut self, count: u32) {
        self.reboots_to_ignore = count;
    }

    pub(crate) fn take_reboot_request(&mut self) -> bool {
        core::mem::take(&mut self.reboot_requested)
    }

    pub(crate) fn configure_from_flash(&mut self) {
        self.reg = [0; 16];
        let base = FPGA_IMAGE_BASE as usize;
        let boots_update = self
            .flash
            .get(base..)
            .is_some_and(|slot| validate_update_image(slot).is_ok());
        self.usr_access = if boots_update {
            FPGA_USR_ACCESS_UPDATE
        } else {
            FPGA_USR_ACCESS_GOLDEN
        };
    }

    pub(crate) fn write_reg(&mut self, a: usize, value: u16) {
        if a < self.reg.len() {
            self.reg[a] = value;
        }
        if a == fw::ADDR_FLASH_CMD as usize {
            self.run(value as u8);
        }
    }

    pub(crate) fn write_buf(&mut self, a: usize, value: u16) {
        self.buf[a & (BUF_WORDS - 1)] = value;
    }

    pub(crate) fn read_reg(&self, a: usize) -> u16 {
        match a as u16 {
            fw::ADDR_FLASH_USR_ACCESS_0 => self.usr_access as u16,
            fw::ADDR_FLASH_USR_ACCESS_1 => (self.usr_access >> 16) as u16,
            _ => self.reg.get(a).copied().unwrap_or(0),
        }
    }

    fn reg24(&self, lo: u16, hi: u16) -> u32 {
        u32::from(self.reg[lo as usize]) | (u32::from(self.reg[hi as usize] & 0xFF) << 16)
    }

    fn run(&mut self, op: u8) {
        let addr = self.reg24(fw::ADDR_FLASH_ADDR_0, fw::ADDR_FLASH_ADDR_1);
        let len = self.reg24(fw::ADDR_FLASH_LEN_0, fw::ADDR_FLASH_LEN_1);
        let end = addr + len;
        let writable = addr >= FPGA_GOLDEN_REGION_END && end <= FPGA_FLASH_BYTES;
        let (err, result) = match op {
            fw::FLASH_OP_READ_ID => (fw::FLASH_ERR_NONE, JEDEC_ID),
            fw::FLASH_OP_CRC32 if end <= FPGA_FLASH_BYTES => {
                let flash = self.flash_mut();
                (
                    fw::FLASH_ERR_NONE,
                    crc32(&flash[addr as usize..end as usize]),
                )
            }
            fw::FLASH_OP_ERASE if !writable => (fw::FLASH_ERR_PROTECTED, 0),
            fw::FLASH_OP_ERASE => {
                let first = (addr - addr % FPGA_SECTOR_BYTES) as usize;
                let last = end.div_ceil(FPGA_SECTOR_BYTES) as usize * FPGA_SECTOR_BYTES as usize;
                if len != 0 {
                    self.flash_mut()[first..last].fill(0xFF);
                }
                (fw::FLASH_ERR_NONE, 0)
            }
            fw::FLASH_OP_PROGRAM if len as usize > fw::FLASH_BUF_BYTES => {
                (fw::FLASH_ERR_INVALID, 0)
            }
            fw::FLASH_OP_PROGRAM if !writable => (fw::FLASH_ERR_PROTECTED, 0),
            fw::FLASH_OP_PROGRAM => {
                let data: Vec<u8> = self.buf.iter().flat_map(|w| w.to_le_bytes()).collect();
                let flash = self.flash_mut();
                for (cell, byte) in flash[addr as usize..end as usize].iter_mut().zip(data) {
                    *cell &= byte;
                }
                (fw::FLASH_ERR_NONE, 0)
            }
            fw::FLASH_OP_REBOOT if self.reboots_to_ignore > 0 => {
                self.reboots_to_ignore -= 1;
                (fw::FLASH_ERR_NONE, 0)
            }
            fw::FLASH_OP_REBOOT => {
                self.reboot_requested = true;
                (fw::FLASH_ERR_NONE, 0)
            }
            _ => (fw::FLASH_ERR_INVALID, 0),
        };
        self.reg[fw::ADDR_FLASH_STATUS as usize] = u16::from(err) << 8;
        self.reg[fw::ADDR_FLASH_RESULT_0 as usize] = result as u16;
        self.reg[fw::ADDR_FLASH_RESULT_1 as usize] = (result >> 16) as u16;
    }
}
