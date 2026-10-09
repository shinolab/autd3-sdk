use autd3_cpu_fw::update::Slot;

use crate::device::Device;
use crate::emu_fpga::{FpgaEmulator, reg};
use crate::fw;

pub trait DeviceTestExt {
    fn power_cycle(&mut self);

    #[must_use]
    fn booted_slot(&self) -> Option<Slot>;
}

impl DeviceTestExt for Device {
    fn power_cycle(&mut self) {
        self.fpga_mut().power_on();
        self.reset_cpu();
    }

    fn booted_slot(&self) -> Option<Slot> {
        self.cpu.booted_slot()
    }
}

pub trait FpgaEmulatorTestExt {
    fn fpga_flash_mut(&mut self) -> &mut [u8];

    fn ignore_next_reboots(&mut self, count: u32);

    fn set_next_sync_edge(&mut self, sys_time_ns: u64);

    fn set_thermal(&mut self, asserted: bool);

    fn set_host_idle_ms(&mut self, ms: Option<u32>);

    #[must_use]
    fn controller_reg(&self, addr: u16) -> u16;

    #[must_use]
    fn req_modulation_bank(&self) -> u16;

    #[must_use]
    fn req_pattern_bank(&self) -> u16;
}

impl FpgaEmulatorTestExt for FpgaEmulator {
    fn fpga_flash_mut(&mut self) -> &mut [u8] {
        self.flash.flash_mut()
    }

    fn ignore_next_reboots(&mut self, count: u32) {
        self.flash.ignore_reboots(count);
    }

    fn set_next_sync_edge(&mut self, sys_time_ns: u64) {
        self.next_sync_edge = sys_time_ns;
    }

    fn set_thermal(&mut self, asserted: bool) {
        self.thermal = asserted;
    }

    fn set_host_idle_ms(&mut self, ms: Option<u32>) {
        self.host_idle_ms = ms;
    }

    fn controller_reg(&self, addr: u16) -> u16 {
        self.controller[addr as usize & 0xFF]
    }

    fn req_modulation_bank(&self) -> u16 {
        self.latched_config[reg(fw::ADDR_MOD_REQ_RD_BANK)]
    }

    fn req_pattern_bank(&self) -> u16 {
        self.latched_config[reg(fw::ADDR_PATTERN_REQ_RD_BANK)]
    }
}
