use autd3_cpu_wire::payload::ForceFanPayload;

use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Encoded, Operation, encode_fixed};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ForceFan {
    pub value: bool,
}

impl crate::sealed::Sealed for ForceFan {}

impl Operation for ForceFan {
    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        Ok(encode_fixed(
            out,
            Cmd::ForceFan,
            &ForceFanPayload { value: self.value },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::encode;

    #[test]
    fn force_fan_encodes_flag() {
        let (cmd, out) = encode(&ForceFan { value: true }).unwrap();
        assert_eq!(
            cmd,
            Encoded::new(Cmd::ForceFan, size_of::<ForceFanPayload>())
        );
        assert_eq!(out[0], 1);

        let (_, out) = encode(&ForceFan { value: false }).unwrap();
        assert_eq!(out[0], 0);
    }
}
