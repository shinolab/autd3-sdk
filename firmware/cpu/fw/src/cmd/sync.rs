use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_ECAT_SYNC_CYCLE_0, ADDR_ECAT_SYNC_CYCLE_1, ADDR_ECAT_SYNC_TIME_0, BRAM_SELECT_CONTROLLER,
    CTL_FLAG_SYNC_SET,
};
use crate::port::Port;
use crate::proto::Error;

const SYNC0_CYCLE_BASE_NS: u32 = 500_000;
const SYS_TIME_NS_PER_TICK: u32 = 3125;

impl Cpu {
    pub(crate) fn sync<P: Port>(&self, port: &mut P) -> Result<(), Error> {
        let cycle_ns = port.sync0_cycle_ns();
        if cycle_ns == 0 || !cycle_ns.is_multiple_of(SYNC0_CYCLE_BASE_NS) {
            return Err(Error::InvalidSync0Cycle);
        }
        let cycle_ticks = (cycle_ns / SYS_TIME_NS_PER_TICK) * 64;

        let next_sync0 = port.next_sync0();
        if next_sync0 == 0 {
            return Err(Error::SyncNotReady);
        }
        fpga::write_u64(port, ADDR_ECAT_SYNC_TIME_0, next_sync0);
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_ECAT_SYNC_CYCLE_0,
            (cycle_ticks & 0xFFFF) as u16,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_ECAT_SYNC_CYCLE_1,
            (cycle_ticks >> 16) as u16,
        );
        self.set_and_wait_update(port, CTL_FLAG_SYNC_SET)
    }
}
