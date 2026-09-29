#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FlashError;

pub trait Port {
    fn fpga_write(&mut self, addr: u16, value: u16);

    fn fpga_read(&mut self, addr: u16) -> u16;

    fn memory_barrier(&mut self);

    fn next_sync_edge(&mut self) -> u64;

    fn sys_time(&mut self) -> u64;

    fn host_idle_ms(&mut self) -> Option<u32>;

    fn flash_read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError>;

    fn flash_write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError>;

    fn flash_erase(&mut self, addr: u32, len: u32) -> Result<(), FlashError>;

    fn reset(&mut self);
}
