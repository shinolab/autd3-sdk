use crate::error::Error;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug, Default)]
pub struct Nop;

impl Operation for Nop {
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
        _out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        Ok(Cmd::Nop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nop_is_no_payload_broadcast() {
        let mut out = [0xAAu8; PAYLOAD_BYTES];
        let cmd = Nop.encode(0, 0, &mut out).unwrap();
        assert_eq!(cmd, Cmd::Nop);
        assert_eq!(Nop.distribution(), Distribution::Broadcast);
        assert_eq!(Nop.frames(), 1);
    }
}
