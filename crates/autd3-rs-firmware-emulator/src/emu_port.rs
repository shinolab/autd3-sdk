use core::num::NonZeroU64;

use crate::emu_fpga::FpgaEmulator;
use autd3_cpu_fw::Port;
use autd3_cpu_fw::port::FlashError;
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

    fn next_sync_edge(&mut self, _guard_ns: u32) -> Option<NonZeroU64> {
        NonZeroU64::new(FpgaEmulator::next_sync_edge(self))
    }

    #[cfg(feature = "udp")]
    fn configure_ptp(&mut self, config: autd3_cpu_fw::ptp::Config) {
        self.note_ptp_config(config);
    }

    #[cfg(not(feature = "udp"))]
    fn configure_ptp(&mut self, _config: autd3_cpu_fw::ptp::Config) {}

    fn set_fpga_bus_wait(&mut self, _wait: autd3_cpu_fw::port::FpgaBusWait) {}

    fn sys_time(&mut self) -> Option<u64> {
        Some(FpgaEmulator::sys_time(self))
    }

    fn host_idle_ms(&mut self) -> Option<u32> {
        FpgaEmulator::host_idle_ms(self)
    }

    fn ptp_unlocked_ms(&mut self) -> Option<u32> {
        None
    }

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
