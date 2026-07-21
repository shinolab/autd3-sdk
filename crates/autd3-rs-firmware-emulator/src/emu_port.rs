use crate::emu_fpga::FpgaEmulator;
use autd3_cpu_fw::Port;

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
}
