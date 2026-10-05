use core::ffi::{c_int, c_ulong};
use core::num::NonZeroU64;

use autd3_cpu_fw::Port;
use autd3_cpu_fw::port::FlashError;
use autd3_cpu_fw::update::{FLASH_BYTES, FLASH_SECTOR_BYTES, LOADER_REGION_END};

use crate::regs::{SYSTEM_PRCR, SYSTEM_SWRR1, dmb, write32};
use crate::udp;

const FPGA_BASE: usize = 0x4400_0000;

unsafe extern "C" {
    fn sflash_read(buf: *mut u8, addr: c_ulong, size: c_int) -> c_int;
    fn sflash_write(buf: *const u8, addr: c_ulong, size: c_int) -> c_int;
    fn sflash_erase_area(addr: c_ulong, size: c_ulong) -> c_int;
    fn sflash_read_command(buf: *mut u8, cmd: c_int, size: c_int) -> c_int;
}

const SFLASH_CMD_RDSR: c_int = 0x05;
const SFLASH_STATUS_WIP: u8 = 0x01;
const FLASH_IDLE_MAX_POLLS: u32 = 2_000_000;
const PRCR_RESET_UNLOCK: u32 = 0x0000_A502;
const SWRR1_SOFTWARE_RESET: u32 = 0x4321_A501;

fn flash_range_ok(addr: u32, len: usize) -> bool {
    (LOADER_REGION_END..=FLASH_BYTES).contains(&addr)
        && u32::try_from(len).is_ok_and(|len| len <= FLASH_BYTES - addr)
}

fn flash_wait_idle() -> Result<(), FlashError> {
    for _ in 0..FLASH_IDLE_MAX_POLLS {
        let mut status = 0u8;
        flash_result(unsafe { sflash_read_command(&raw mut status, SFLASH_CMD_RDSR, 1) })?;
        if status & SFLASH_STATUS_WIP == 0 {
            return Ok(());
        }
    }
    Err(FlashError)
}

fn flash_result(code: c_int) -> Result<(), FlashError> {
    if code == 0 { Ok(()) } else { Err(FlashError) }
}

pub(crate) struct HwPort;

impl Port for HwPort {
    fn fpga_write(&mut self, addr: u16, value: u16) {
        unsafe {
            (FPGA_BASE as *mut u16)
                .add(addr as usize)
                .write_volatile(value);
        }
    }

    fn fpga_read(&mut self, addr: u16) -> u16 {
        unsafe { (FPGA_BASE as *const u16).add(addr as usize).read_volatile() }
    }

    fn memory_barrier(&mut self) {
        dmb();
    }

    fn next_sync_edge(&mut self, guard_ns: u32) -> Option<NonZeroU64> {
        udp::next_sync_edge(guard_ns)
    }

    fn configure_ptp(&mut self, config: autd3_cpu_fw::ptp::Config) {
        udp::configure_ptp(config);
    }

    fn set_fpga_bus_wait(&mut self, wait: autd3_cpu_fw::port::FpgaBusWait) {
        dmb();
        udp::set_fpga_bus_wait(u32::from(wait.as_u8()));
        dmb();
    }

    fn sys_time(&mut self) -> Option<u64> {
        udp::sys_time()
    }

    fn host_idle_ms(&mut self) -> Option<u32> {
        udp::host_idle_ms()
    }

    fn ptp_unlocked_ms(&mut self) -> Option<u32> {
        udp::ptp_unlocked_ms()
    }

    fn flash_read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError> {
        if !flash_range_ok(addr, buf.len()) {
            return Err(FlashError);
        }
        let size = c_int::try_from(buf.len()).map_err(|_| FlashError)?;
        flash_wait_idle()?;
        flash_result(unsafe { sflash_read(buf.as_mut_ptr(), c_ulong::from(addr), size) })
    }

    fn flash_write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError> {
        if !flash_range_ok(addr, data.len()) {
            return Err(FlashError);
        }
        let size = c_int::try_from(data.len()).map_err(|_| FlashError)?;
        flash_result(unsafe { sflash_write(data.as_ptr(), c_ulong::from(addr), size) })
    }

    fn flash_erase(&mut self, addr: u32, len: u32) -> Result<(), FlashError> {
        if !flash_range_ok(addr, len as usize)
            || !addr.is_multiple_of(FLASH_SECTOR_BYTES)
            || !len.is_multiple_of(FLASH_SECTOR_BYTES)
        {
            return Err(FlashError);
        }
        flash_result(unsafe { sflash_erase_area(c_ulong::from(addr), c_ulong::from(len)) })
    }

    fn reset(&mut self) {
        write32(SYSTEM_PRCR, PRCR_RESET_UNLOCK);
        write32(SYSTEM_SWRR1, SWRR1_SOFTWARE_RESET);
        loop {
            core::hint::spin_loop();
        }
    }
}
