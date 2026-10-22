use crate::datagram::Datagram;
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::Cmd;

use super::{Operation, set_cmd};

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
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        set_cmd(out, Cmd::Clear, 0);
        Ok(())
    }
}

impl crate::sealed::Sealed for Nop {}

impl Operation for Nop {
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        set_cmd(out, Cmd::Nop, 0);
        Ok(())
    }
}

impl crate::sealed::Sealed for Synchronize {}

impl Operation for Synchronize {
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        set_cmd(out, Cmd::Synchronize, 0);
        Ok(())
    }
}

impl crate::sealed::Sealed for ReleaseFailsafe {}

impl Operation for ReleaseFailsafe {
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        set_cmd(out, Cmd::ReleaseFailsafe, 0);
        Ok(())
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
            let mut out = Datagram::no_payload(Cmd::Nop);
            op.encode(&test_device(0), &mut out).unwrap();
            assert_eq!((out.cmd, out.payload_len), (cmd, 0));
            assert_eq!(op.distribution(), Distribution::Broadcast);
        }
    }
}
