use crate::fpga;
use crate::params::BRAM_SELECT_OUTPUT_MASK;
use crate::port::Port;
use crate::proto::OUTPUT_MASK_WORDS;

pub(crate) fn mute<P: Port>(port: &mut P) {
    for j in 0..OUTPUT_MASK_WORDS {
        fpga::write(port, BRAM_SELECT_OUTPUT_MASK, j as u16, 0);
    }
}
