use crate::fpga;
use crate::params::{BRAM_CNT_SELECT_OUTPUT_MASK, BRAM_SELECT_CONTROLLER};
use crate::port::Port;
use crate::proto::OUTPUT_MASK_WORDS;

pub(crate) fn mute<P: Port>(port: &mut P) {
    for j in 0..OUTPUT_MASK_WORDS {
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            (u16::from(BRAM_CNT_SELECT_OUTPUT_MASK) << 8) | j as u16,
            0,
        );
    }
}
