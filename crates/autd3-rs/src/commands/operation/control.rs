use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Encoded, Operation};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clear;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Nop;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Synchronize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReleaseFailsafe;

impl crate::sealed::Sealed for Clear {}

impl Operation for Clear {
    fn encode(&self, _device: &Device, _out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        Ok(Encoded::no_payload(Cmd::Clear))
    }
}

impl crate::sealed::Sealed for Nop {}

impl Operation for Nop {
    fn encode(&self, _device: &Device, _out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        Ok(Encoded::no_payload(Cmd::Nop))
    }
}

impl crate::sealed::Sealed for Synchronize {}

impl Operation for Synchronize {
    fn encode(&self, _device: &Device, _out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        Ok(Encoded::no_payload(Cmd::Synchronize))
    }
}

impl crate::sealed::Sealed for ReleaseFailsafe {}

impl Operation for ReleaseFailsafe {
    fn encode(&self, _device: &Device, _out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        Ok(Encoded::no_payload(Cmd::ReleaseFailsafe))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::operation::Distribution;
    use crate::test_utils::test_device;

    #[test]
    fn control_ops_are_no_payload_broadcast() {
        let ops: [(&dyn Operation, Cmd); 4] = [
            (&Clear, Cmd::Clear),
            (&Nop, Cmd::Nop),
            (&Synchronize, Cmd::Synchronize),
            (&ReleaseFailsafe, Cmd::ReleaseFailsafe),
        ];
        for (op, cmd) in ops {
            let mut out = [0xAAu8; PAYLOAD_BYTES];
            assert_eq!(
                op.encode(&test_device(0), &mut out).unwrap(),
                Encoded::no_payload(cmd)
            );
            assert_eq!(op.distribution(), Distribution::Broadcast);
        }
    }
}
