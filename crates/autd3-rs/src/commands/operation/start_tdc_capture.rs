use autd3_cpu_wire::payload::{StartTdcCapturePayload, TdcSource};

use crate::datagram::Datagram;
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::Cmd;

use super::{Operation, encode_header_only};

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartTdcCapture {
    pub source: TdcSource,
}

impl crate::sealed::Sealed for StartTdcCapture {}

impl Operation for StartTdcCapture {
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        encode_header_only(
            out,
            Cmd::StartTdcCapture,
            &StartTdcCapturePayload {
                source: self.source,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::operation::Distribution;
    use crate::test_utils::test_device;

    #[test]
    fn the_source_is_carried_in_the_only_payload_byte() {
        for source in [TdcSource::SyncIn, TdcSource::Calibration] {
            let op = StartTdcCapture { source };
            let mut out = Datagram::no_payload(Cmd::Nop);
            op.encode(&test_device(0), &mut out).unwrap();
            assert_eq!(out.cmd, Cmd::StartTdcCapture);
            assert_eq!(out.payload(), [source.as_u8()]);
            assert_eq!(op.distribution(), Distribution::Broadcast);
        }
    }
}
