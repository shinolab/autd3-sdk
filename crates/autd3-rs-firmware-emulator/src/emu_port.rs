use crate::emu_fpga::FpgaEmulator;
use autd3_cpu_fw::Port;
use autd3_cpu_fw::port::FlashError;
use autd3_cpu_fw::proto::TxFrame;
use autd3_cpu_fw::update::{FLASH_SECTOR_BYTES, LOADER_REGION_END};

fn flash_span(
    flash_len: usize,
    addr: u32,
    len: usize,
) -> Result<core::ops::Range<usize>, FlashError> {
    let start = usize::try_from(addr).map_err(|_| FlashError)?;
    let end = start.checked_add(len).ok_or(FlashError)?;
    if start < LOADER_REGION_END as usize || end > flash_len {
        return Err(FlashError);
    }
    Ok(start..end)
}

impl Port for FpgaEmulator {
    fn fpga_write(&mut self, addr: u16, value: u16) {
        self.write(addr, value);
    }

    fn fpga_read(&mut self, addr: u16) -> u16 {
        self.read(addr)
    }

    fn memory_barrier(&mut self) {}

    fn next_sync0(&mut self) -> u64 {
        FpgaEmulator::next_sync0(self)
    }

    fn dc_sys_time(&mut self) -> u64 {
        FpgaEmulator::dc_sys_time(self)
    }

    fn sync0_cycle_ns(&mut self) -> u32 {
        FpgaEmulator::sync0_cycle_ns(self)
    }

    fn al_status_code(&mut self) -> u16 {
        FpgaEmulator::al_status_code(self)
    }

    fn publish_tx(&mut self, _tx: TxFrame) {}

    fn flash_read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError> {
        let start = usize::try_from(addr).map_err(|_| FlashError)?;
        let end = start.checked_add(buf.len()).ok_or(FlashError)?;
        let flash = self.cpu_flash();
        if end > flash.len() {
            return Err(FlashError);
        }
        buf.copy_from_slice(&flash[start..end]);
        Ok(())
    }

    fn flash_write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError> {
        let span = flash_span(self.cpu_flash().len(), addr, data.len())?;
        for (cell, &byte) in self.cpu_flash_mut()[span].iter_mut().zip(data) {
            *cell &= byte;
        }
        Ok(())
    }

    fn flash_erase(&mut self, addr: u32, len: u32) -> Result<(), FlashError> {
        if !addr.is_multiple_of(FLASH_SECTOR_BYTES) || !len.is_multiple_of(FLASH_SECTOR_BYTES) {
            return Err(FlashError);
        }
        let span = flash_span(self.cpu_flash().len(), addr, len as usize)?;
        self.cpu_flash_mut()[span].fill(0xFF);
        Ok(())
    }

    fn reset(&mut self) {
        self.note_reset();
    }
}
