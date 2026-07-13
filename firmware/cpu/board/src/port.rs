use core::arch::asm;

use autd3_cpu_fw::Port;

use crate::bsp;
use crate::regs::{
    ECATC_AL_STATUS_CODE, ECATC_DC_CYC_START_TIME_HI, ECATC_DC_CYC_START_TIME_LO,
    ECATC_DC_SYNC0_CYC_TIME, ECATC_DC_SYS_TIME_HI, ECATC_DC_SYS_TIME_LO, read16, read32,
};

const FPGA_BASE: usize = 0x4400_0000;

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
        // SAFETY: the FPGA is mapped at CS1 (`FPGA_BASE`) as a 16-bit bus. `addr` is a
        // 16-bit word index, so the access stays inside the 128 KiB aperture and is
        // naturally aligned.
        unsafe {
            (FPGA_BASE as *mut u16)
                .add(addr as usize)
                .write_volatile(value);
        }
    }

    fn fpga_read(&mut self, addr: u16) -> u16 {
        // SAFETY: see `fpga_write`.
        unsafe { (FPGA_BASE as *const u16).add(addr as usize).read_volatile() }
    }

    fn memory_barrier(&mut self) {
        // SAFETY: `dmb` has no operands. The implicit memory clobber prevents the compiler
        // from moving FPGA accesses across the barrier, which the bank/page switch
        // sequences depend on.
        unsafe { asm!("dmb", options(nostack, preserves_flags)) };
    }

    fn sleep_ms(&mut self, ms: u16) {
        bsp::timer::delay_ms(ms);
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
}
