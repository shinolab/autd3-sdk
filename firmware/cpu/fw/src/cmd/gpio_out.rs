use crate::app::Cpu;
use crate::fpga;
use crate::params::{ADDR_DEBUG_VALUE0_0, BRAM_SELECT_CONTROLLER, CTL_FLAG_DEBUG_SET};
use crate::port::Port;
use crate::proto::{GPIO_OUT_NUM, GPIO_OUT_OFFSET_DATA};

const GPIO_OUT_WORDS: usize = GPIO_OUT_NUM * 4;

impl Cpu {
    pub(crate) fn gpio_out<P: Port>(&self, port: &mut P, payload: &[u8]) -> u8 {
        let data = &payload[GPIO_OUT_OFFSET_DATA..];
        for i in 0..GPIO_OUT_WORDS {
            let value = u16::from_le_bytes([data[2 * i], data[2 * i + 1]]);
            fpga::write(
                port,
                BRAM_SELECT_CONTROLLER,
                ADDR_DEBUG_VALUE0_0 + i as u16,
                value,
            );
        }
        self.set_and_wait_update(port, CTL_FLAG_DEBUG_SET)
    }
}
