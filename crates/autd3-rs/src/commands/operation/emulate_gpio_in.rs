use autd3_cpu_wire::payload::GpioInPayload;

use crate::datagram::Datagram;
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::Cmd;

use super::{Operation, encode_header_only};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct EmulateGpioIn {
    pub values: [bool; 4],
}

impl crate::sealed::Sealed for EmulateGpioIn {}

impl Operation for EmulateGpioIn {
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        encode_header_only(
            out,
            Cmd::EmulateGpioIn,
            &GpioInPayload {
                values: self.values,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::encode;

    #[test]
    fn gpio_in_lays_out_values() {
        let (cmd, out) = encode(&EmulateGpioIn {
            values: [false, true, false, true],
        })
        .unwrap();
        assert_eq!(cmd, (Cmd::EmulateGpioIn, size_of::<GpioInPayload>()));
        assert_eq!(&out[..4], &[0, 1, 0, 1]);
    }
}
