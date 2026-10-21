use core::num::NonZeroU64;

use autd3_cpu_fw::Port;
use autd3_cpu_fw::port::FlashError;
use autd3_cpu_fw::update::{FLASH_BYTES, FLASH_SECTOR_BYTES, LOADER_REGION_END};

use crate::regs::{SYSTEM_PRCR, SYSTEM_SWRR1, dmb, write32};
use crate::{sflash, udp};

const FPGA_BASE: usize = 0x4400_0000;

const PRCR_RESET_UNLOCK: u32 = 0x0000_A502;
const SWRR1_SOFTWARE_RESET: u32 = 0x4321_A501;

fn flash_range_ok(addr: u32, len: usize) -> bool {
    (LOADER_REGION_END..=FLASH_BYTES).contains(&addr)
        && u32::try_from(len).is_ok_and(|len| len <= FLASH_BYTES - addr)
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

    fn without_irq<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        udp::irq::without_irq(|| f(self))
    }

    fn flash_read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError> {
        if !flash_range_ok(addr, buf.len()) {
            return Err(FlashError);
        }
        sflash::read(addr, buf)
    }

    fn flash_write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError> {
        if !flash_range_ok(addr, data.len()) {
            return Err(FlashError);
        }
        sflash::write(addr, data)
    }

    fn flash_erase(&mut self, addr: u32, len: u32) -> Result<(), FlashError> {
        if !flash_range_ok(addr, len as usize)
            || !addr.is_multiple_of(FLASH_SECTOR_BYTES)
            || !len.is_multiple_of(FLASH_SECTOR_BYTES)
        {
            return Err(FlashError);
        }
        sflash::erase(addr, len)
    }

    fn reset(&mut self) {
        write32(SYSTEM_PRCR, PRCR_RESET_UNLOCK);
        write32(SYSTEM_SWRR1, SWRR1_SOFTWARE_RESET);
        loop {
            core::hint::spin_loop();
        }
    }
}
