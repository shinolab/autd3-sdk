use core::num::NonZeroU64;

pub use autd3_cpu_wire::config::FpgaBusWait;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FlashError;

pub trait Port {
    fn fpga_write(&mut self, addr: u16, value: u16);

    fn fpga_read(&mut self, addr: u16) -> u16;

    fn memory_barrier(&mut self);

    fn next_sync_edge(&mut self, guard_ns: u32) -> Option<NonZeroU64>;

    fn configure_ptp(&mut self, config: autd3_cpu_wire::config::PtpConfig);

    fn set_fpga_bus_wait(&mut self, wait: FpgaBusWait);

    fn sys_time(&mut self) -> Option<u64>;

    fn host_idle_ms(&mut self) -> Option<u32>;

    fn ptp_unlocked_ms(&mut self) -> Option<u32>;

    fn flash_read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError>;

    fn flash_write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError>;

    fn flash_erase(&mut self, addr: u32, len: u32) -> Result<(), FlashError>;

    fn reset(&mut self);
}
