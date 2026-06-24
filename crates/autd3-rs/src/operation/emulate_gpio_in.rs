use crate::error::Error;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct EmulateGpioIn {
    pub values: [bool; 4],
}

impl Operation for EmulateGpioIn {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(
        &self,
        _device: usize,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        let mut flag = 0u8;
        for (bit, &on) in self.values.iter().enumerate() {
            if on {
                flag |= 1 << bit;
            }
        }
        out[0] = flag;
        Ok(Cmd::EmulateGpioIn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpio_in_packs_bits() {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = EmulateGpioIn {
            values: [false, true, false, true],
        }
        .encode(0, 0, &mut out)
        .unwrap();
        assert_eq!(cmd, Cmd::EmulateGpioIn);
        assert_eq!(out[0], 0b1010);
    }
}
