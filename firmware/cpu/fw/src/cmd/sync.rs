use autd3_cpu_wire::udp::SYNC_CYCLE_NS;

use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_SYNC_CYCLE_0, ADDR_SYNC_CYCLE_1, ADDR_SYNC_TIME_0, BRAM_SELECT_CONTROLLER,
    CTL_FLAG_SYNC_SET,
};
use crate::port::Port;
use crate::proto::Error;

const SYS_TIME_NS_PER_TICK: u32 = 3125;
const SYNC_CYCLE_TICKS: u32 = (SYNC_CYCLE_NS / SYS_TIME_NS_PER_TICK) * 64;

impl Cpu {
    pub(crate) fn sync<P: Port>(&self, port: &mut P) -> Result<(), Error> {
        let next_edge = port.next_sync_edge();
        if next_edge == 0 {
            return Err(Error::SyncNotReady);
        }
        fpga::write_u64(port, ADDR_SYNC_TIME_0, next_edge);
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_SYNC_CYCLE_0,
            (SYNC_CYCLE_TICKS & 0xFFFF) as u16,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_SYNC_CYCLE_1,
            (SYNC_CYCLE_TICKS >> 16) as u16,
        );
        self.set_and_wait_update(port, CTL_FLAG_SYNC_SET)
    }
}
