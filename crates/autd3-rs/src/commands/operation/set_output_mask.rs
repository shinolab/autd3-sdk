use autd3_cpu_wire::payload::OutputMaskPayload;

use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Encoded, Operation, device_slot, encode_fixed};

#[derive(Clone, Copy, Debug)]
pub struct SetOutputMask<'a> {
    pub masks: &'a [Vec<bool>],
}

impl crate::sealed::Sealed for SetOutputMask<'_> {}

impl Operation for SetOutputMask<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        Ok(encode_fixed(
            out,
            Cmd::SetOutputMask,
            &OutputMaskPayload::new(device_slot(self.masks, device)?),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PayloadError;
    use crate::test_utils::{encode, test_device};

    #[test]
    fn output_mask_packs_bits_lsb_first() {
        let dev = test_device(0);
        let n = dev.num_transducers();
        let mut mask = vec![false; n];
        mask[0] = true;
        mask[3] = true;
        mask[8] = true;
        mask[16] = true;
        mask[n - 1] = true;
        let data = vec![mask];
        let (cmd, out) = encode(&SetOutputMask { masks: &data }).unwrap();
        assert_eq!(
            cmd,
            Encoded::new(Cmd::SetOutputMask, size_of::<OutputMaskPayload>())
        );
        let mut expected = [0u8; size_of::<OutputMaskPayload>()];
        expected[0] = 0b0000_1001;
        expected[1] = 0b0000_0001;
        expected[2] = 0b0000_0001;
        expected[(n - 1) / 8] |= 1 << ((n - 1) % 8);
        assert_eq!(out[..expected.len()], expected);
    }

    #[test]
    fn output_mask_rejects_device_out_of_range() {
        let dev = test_device(1);
        let data = vec![vec![true; dev.num_transducers()]];
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            SetOutputMask { masks: &data }.encode(&dev, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn output_mask_rejects_wrong_transducer_count() {
        let dev = test_device(0);
        let data = vec![vec![true; dev.num_transducers() + 1]];
        assert!(matches!(
            encode(&SetOutputMask { masks: &data }),
            Err(Error::InvalidPayload(
                PayloadError::TransducerCountMismatch { .. }
            ))
        ));
    }
}
