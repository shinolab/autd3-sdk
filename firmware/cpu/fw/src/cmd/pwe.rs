use crate::fpga::{self, PWE_TABLE_SIZE};
use crate::params::BRAM_SELECT_PWE_TABLE;
use crate::port::Port;
use crate::proto::{ERR_NONE, PWE_OFFSET_DATA};

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> u8 {
    let data = &payload[PWE_OFFSET_DATA..];
    for i in 0..PWE_TABLE_SIZE {
        let value = u16::from_le_bytes([data[2 * i], data[2 * i + 1]]);
        fpga::write(port, BRAM_SELECT_PWE_TABLE, i as u16, value);
    }
    ERR_NONE
}
