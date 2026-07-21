use crate::error::Error;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ForceFan {
    pub value: bool,
}

impl Operation for ForceFan {
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
        out[0] = u8::from(self.value);
        Ok(Cmd::ForceFan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_fan_encodes_flag() {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = ForceFan { value: true }.encode(0, 0, &mut out).unwrap();
        assert_eq!(cmd, Cmd::ForceFan);
        assert_eq!(out[0], 1);

        let mut out = [0u8; PAYLOAD_BYTES];
        ForceFan { value: false }.encode(0, 0, &mut out).unwrap();
        assert_eq!(out[0], 0);
    }
}
