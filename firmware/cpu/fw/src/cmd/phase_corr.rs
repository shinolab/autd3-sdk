use crate::fpga::{self, PHASE_CORR_WORDS};
use crate::params::{BRAM_CNT_SELECT_PHASE_CORR, BRAM_SELECT_CONTROLLER, NUM_TRANSDUCERS};
use crate::port::Port;
use crate::proto::{ERR_NONE, PHASE_CORR_OFFSET_DATA};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let data = &payload[PHASE_CORR_OFFSET_DATA..];
    for j in 0..PHASE_CORR_WORDS {
        let lo = u16::from(data[2 * j]);
        let hi = if 2 * j + 1 < NUM_TRANSDUCERS {
            u16::from(data[2 * j + 1])
        } else {
            0
        };
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            (u16::from(BRAM_CNT_SELECT_PHASE_CORR) << 8) | j as u16,
            lo | (hi << 8),
        );
    }
    ERR_NONE
}
