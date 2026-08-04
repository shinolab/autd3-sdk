use autd3_cpu_wire::payload::OutputMaskPayload;
use zerocopy::FromBytes;

use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug)]
pub struct SetOutputMask<'a> {
    pub masks: &'a [Vec<bool>],
}

impl crate::sealed::Sealed for SetOutputMask<'_> {}

impl Operation for SetOutputMask<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        let mask = self
            .masks
            .get(device.idx())
            .ok_or(PayloadError::EmissionsDeviceOutOfRange {
                device: device.idx(),
                len: self.masks.len(),
            })?;
        if mask.len() != device.num_transducers() {
            return Err(PayloadError::TransducerCountMismatch {
                device: device.idx(),
                got: mask.len(),
                expected: device.num_transducers(),
            }
            .into());
        }
        let (p, _) = OutputMaskPayload::mut_from_prefix(&mut out[..]).unwrap();
        mask.iter()
            .zip(p.data.iter_mut())
            .for_each(|(&on, dst)| *dst = u8::from(on));
        Ok(Cmd::SetOutputMask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;

    #[test]
    fn output_mask_writes_bytes() {
        let dev = test_device(0);
        let n = dev.num_transducers();
        let mut mask = vec![false; n];
        mask[0] = true;
        mask[3] = true;
        mask[8] = true;
        mask[n - 1] = true;
        let data = vec![mask];
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = SetOutputMask { masks: &data }
            .encode(&dev, &mut out)
            .unwrap();
        assert_eq!(cmd, Cmd::SetOutputMask);
        assert_eq!(out[0], 1);
        assert_eq!(out[1], 0);
        assert_eq!(out[3], 1);
        assert_eq!(out[8], 1);
        assert_eq!(out[n - 1], 1);
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
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            SetOutputMask { masks: &data }.encode(&dev, &mut out),
            Err(Error::InvalidPayload(
                PayloadError::TransducerCountMismatch { .. }
            ))
        ));
    }
}
