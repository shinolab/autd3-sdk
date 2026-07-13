use crate::fpga;
use crate::params::{BRAM_CNT_SELECT_OUTPUT_MASK, BRAM_SELECT_CONTROLLER};
use crate::port::Port;
use crate::proto::{ERR_NONE, OUTPUT_MASK_OFFSET_DATA, OUTPUT_MASK_WORDS};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let data = &payload[OUTPUT_MASK_OFFSET_DATA..];
    for j in 0..OUTPUT_MASK_WORDS {
        let value = u16::from_le_bytes([data[2 * j], data[2 * j + 1]]);
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            (u16::from(BRAM_CNT_SELECT_OUTPUT_MASK) << 8) | j as u16,
            value,
        );
    }
    ERR_NONE
}
