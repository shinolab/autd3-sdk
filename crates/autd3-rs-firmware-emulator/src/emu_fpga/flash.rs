use autd3_cpu_fw::fpga_update::{
    FPGA_FLASH_BYTES, FPGA_GOLDEN_REGION_END, FPGA_IMAGE_BASE, FPGA_SECTOR_BYTES,
    FPGA_USR_ACCESS_GOLDEN, FPGA_USR_ACCESS_UPDATE, validate_update_image,
};
use autd3_cpu_fw::update::crc32;

use crate::fw;
use crate::fw::{FlashErr, FlashOp};

const BUF_WORDS: usize = fw::FLASH_BUF_BYTES / 2;

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
        (u32::from(self.reg[hi as usize] & 0xFF) << 16) | u32::from(self.reg[lo as usize])
    }

    fn run(&mut self, op: u8) {
        let addr = self.reg24(fw::ADDR_FLASH_ADDR_0, fw::ADDR_FLASH_ADDR_1);
        let len = self.reg24(fw::ADDR_FLASH_LEN_0, fw::ADDR_FLASH_LEN_1);
        let end = addr + len;
        let writable = addr >= FPGA_GOLDEN_REGION_END && end <= FPGA_FLASH_BYTES;
        let (err, result) = match FlashOp::from_u8(op) {
            Some(FlashOp::Crc32) if end <= FPGA_FLASH_BYTES => {
                let flash = self.flash_mut();
                (FlashErr::None, crc32(&flash[addr as usize..end as usize]))
            }
            Some(FlashOp::Erase) if !writable => (FlashErr::Protected, 0),
            Some(FlashOp::Erase) => {
                let first = (addr - addr % FPGA_SECTOR_BYTES) as usize;
                let last = end.div_ceil(FPGA_SECTOR_BYTES) as usize * FPGA_SECTOR_BYTES as usize;
                if len != 0 {
                    self.flash_mut()[first..last].fill(0xFF);
                }
                (FlashErr::None, 0)
            }
            Some(FlashOp::Program) if len as usize > fw::FLASH_BUF_BYTES => (FlashErr::Invalid, 0),
            Some(FlashOp::Program) if !writable => (FlashErr::Protected, 0),
            Some(FlashOp::Program) => {
                let data: Vec<u8> = self.buf.iter().flat_map(|w| w.to_le_bytes()).collect();
                let flash = self.flash_mut();
                for (cell, byte) in flash[addr as usize..end as usize].iter_mut().zip(data) {
                    *cell &= byte;
                }
                (FlashErr::None, 0)
            }
            Some(FlashOp::Reboot) if self.reboots_to_ignore > 0 => {
                self.reboots_to_ignore -= 1;
                (FlashErr::None, 0)
            }
            Some(FlashOp::Reboot) => {
                self.reboot_requested = true;
                (FlashErr::None, 0)
            }
            _ => (FlashErr::Invalid, 0),
        };
        self.reg[fw::ADDR_FLASH_STATUS as usize] = u16::from(err.as_u8()) << 8;
        self.reg[fw::ADDR_FLASH_RESULT_0 as usize] = result as u16;
        self.reg[fw::ADDR_FLASH_RESULT_1 as usize] = (result >> 16) as u16;
    }
}

const _: () = assert!(FlashOp::from_u8(0x01).is_none());
const _: () = assert!(FlashOp::Crc32.as_u8() == 0x02);
const _: () = assert!(FlashOp::Erase.as_u8() == 0x03);
const _: () = assert!(FlashOp::Program.as_u8() == 0x04);
const _: () = assert!(FlashOp::Reboot.as_u8() == 0x05);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_op_0x01_is_invalid() {
        let mut flash = FlashEmulator::new();
        flash.write_reg(fw::ADDR_FLASH_LEN_0 as usize, 1);
        flash.write_reg(fw::ADDR_FLASH_CMD as usize, FlashOp::Crc32.as_u8().into());
        assert_eq!(0, flash.read_reg(fw::ADDR_FLASH_STATUS as usize));
        assert_ne!(0, flash.read_reg(fw::ADDR_FLASH_RESULT_1 as usize));

        flash.write_reg(fw::ADDR_FLASH_CMD as usize, 0x01);
        assert_eq!(
            u16::from(FlashErr::Invalid.as_u8()) << 8,
            flash.read_reg(fw::ADDR_FLASH_STATUS as usize)
        );
        assert_eq!(0, flash.read_reg(fw::ADDR_FLASH_RESULT_0 as usize));
        assert_eq!(0, flash.read_reg(fw::ADDR_FLASH_RESULT_1 as usize));
    }
}
