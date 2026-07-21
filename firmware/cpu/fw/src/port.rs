pub trait Port {
    fn fpga_write(&mut self, addr: u16, value: u16);

    fn fpga_read(&mut self, addr: u16) -> u16;

    fn memory_barrier(&mut self);

    fn next_sync0(&mut self) -> u64;

    fn dc_sys_time(&mut self) -> u64;

    fn sync0_cycle_ns(&mut self) -> u32;

    fn al_status_code(&mut self) -> u16;
}
