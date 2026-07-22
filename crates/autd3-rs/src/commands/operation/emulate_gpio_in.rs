use autd3_cpu_wire::payload::GpioInPayload;
use zerocopy::FromBytes;

use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct EmulateGpioIn {
    pub values: [bool; 4],
}

impl Operation for EmulateGpioIn {
    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        let (p, _) = GpioInPayload::mut_from_prefix(&mut out[..]).unwrap();
        *p = GpioInPayload {
            gpio_in_0: u8::from(self.values[0]),
            gpio_in_1: u8::from(self.values[1]),
            gpio_in_2: u8::from(self.values[2]),
            gpio_in_3: u8::from(self.values[3]),
        };
        Ok(Cmd::EmulateGpioIn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;

    #[test]
    fn gpio_in_lays_out_values() {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = EmulateGpioIn {
            values: [false, true, false, true],
        }
        .encode(&test_device(0), &mut out)
        .unwrap();
        assert_eq!(cmd, Cmd::EmulateGpioIn);
        assert_eq!(&out[..4], &[0, 1, 0, 1]);
    }
}
