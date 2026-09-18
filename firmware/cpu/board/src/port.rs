use core::arch::asm;
use core::ffi::{c_int, c_ulong};

use autd3_cpu_fw::Port;
use autd3_cpu_fw::port::FlashError;
use autd3_cpu_fw::proto::TxFrame;
use autd3_cpu_fw::update::{FLASH_BYTES, FLASH_SECTOR_BYTES, LOADER_REGION_END};

use crate::regs::{
    ECATC_AL_STATUS_CODE, ECATC_DC_CYC_START_TIME_HI, ECATC_DC_CYC_START_TIME_LO,
    ECATC_DC_SYNC0_CYC_TIME, ECATC_DC_SYS_TIME_HI, ECATC_DC_SYS_TIME_LO, SYSTEM_PRCR, SYSTEM_SWRR1,
    read16, read32, write32,
};

const FPGA_BASE: usize = 0x4400_0000;

#[repr(C)]
struct TxWire {
    _reserved: u16,
    ack_data: u16,
}

unsafe extern "C" {
    static mut _sTx: TxWire;
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

const MICROSECONDS: u64 = 1000;
const SYNC0_GUARD_NS: u64 = 250 * MICROSECONDS;
const SYNC0_MAX_POLLS: u32 = 1_000_000;

fn read_dc_u64(lo: usize, hi: usize) -> u64 {
    loop {
        let low = read32(lo);
        let high = read32(hi);
        let low2 = read32(lo);
        if low2 >= low {
            return (u64::from(high) << 32) | u64::from(low);
        }
    }
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
        unsafe { asm!("dmb", options(nostack, preserves_flags)) };
    }

    fn next_sync0(&mut self) -> u64 {
        let mut next_sync0 = read_dc_u64(ECATC_DC_CYC_START_TIME_LO, ECATC_DC_CYC_START_TIME_HI);
        if next_sync0 == 0 {
            return 0;
        }
        let mut sys_time = read_dc_u64(ECATC_DC_SYS_TIME_LO, ECATC_DC_SYS_TIME_HI);
        let mut guard = 0u32;
        while next_sync0 < sys_time + SYNC0_GUARD_NS {
            guard += 1;
            if guard > SYNC0_MAX_POLLS {
                return 0;
            }
            sys_time = read_dc_u64(ECATC_DC_SYS_TIME_LO, ECATC_DC_SYS_TIME_HI);
            if sys_time > next_sync0 {
                next_sync0 = read_dc_u64(ECATC_DC_CYC_START_TIME_LO, ECATC_DC_CYC_START_TIME_HI);
            }
        }
        next_sync0
    }

    fn dc_sys_time(&mut self) -> u64 {
        read_dc_u64(ECATC_DC_SYS_TIME_LO, ECATC_DC_SYS_TIME_HI)
    }

    fn sync0_cycle_ns(&mut self) -> u32 {
        read32(ECATC_DC_SYNC0_CYC_TIME)
    }

    fn al_status_code(&mut self) -> u16 {
        read16(ECATC_AL_STATUS_CODE)
    }

    fn publish_tx(&mut self, tx: TxFrame) {
        let packed = u16::from(tx.ack) | (u16::from(tx.data) << 8);
        unsafe { (&raw mut _sTx.ack_data).write_volatile(packed) };
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
