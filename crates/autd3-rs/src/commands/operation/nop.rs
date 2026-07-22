use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug, Default)]
pub struct Nop;

impl Operation for Nop {
    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(&self, _device: &Device, _out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        Ok(Cmd::Nop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;

    #[test]
    fn nop_is_no_payload_broadcast() {
        let mut out = [0xAAu8; PAYLOAD_BYTES];
        let cmd = Nop.encode(&test_device(0), &mut out).unwrap();
        assert_eq!(cmd, Cmd::Nop);
        assert_eq!(Nop.distribution(), Distribution::Broadcast);
    }
}
