use autd3_cpu_wire::udp::SYNC_CYCLE_NS;

use crate::app::Cpu;
use crate::fpga::{self, sys_time_ticks};
use crate::params::{ADDR_SYNC_TIME_0, CTL_FLAG_SYNC_SET, SYNC_CYCLE_TICKS};
use crate::port::Port;
use crate::proto::Error;

const _: () = assert!(sys_time_ticks(SYNC_CYCLE_NS as u64) == SYNC_CYCLE_TICKS as u64);

impl Cpu {
    pub(crate) fn sync<P: Port>(&self, port: &mut P) -> Result<(), Error> {
        let next_edge = port.next_sync_edge();
        if next_edge == 0 {
            return Err(Error::SyncNotReady);
        }
        fpga::write_u64(port, ADDR_SYNC_TIME_0, sys_time_ticks(next_edge));
        self.set_and_wait_update(port, CTL_FLAG_SYNC_SET)
    }
}
