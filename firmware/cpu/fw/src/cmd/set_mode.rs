use crate::app::Cpu;
use crate::proto::{ERR_INVALID_PAYLOAD, ERR_NONE, MODE_LOW_LATENCY, SET_MODE_OFFSET_MODE};

impl Cpu {
    pub(crate) fn set_mode_cmd(&self, payload: &[u8]) -> u8 {
        let mode = payload[SET_MODE_OFFSET_MODE];
        if mode > MODE_LOW_LATENCY {
            return ERR_INVALID_PAYLOAD;
        }
        self.set_mode(mode);
        ERR_NONE
    }
}
