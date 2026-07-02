use crate::error::{Error, PayloadError};
use crate::geometry::Autd3;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug)]
pub struct SetOutputMask<'a> {
    pub masks: &'a [Vec<bool>],
}

impl Operation for SetOutputMask<'_> {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(
        &self,
        device: usize,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        let mask = self.masks.get(device).ok_or(Error::InvalidPayload(
            PayloadError::EmissionsDeviceOutOfRange {
                device,
                len: self.masks.len(),
            },
        ))?;
        if mask.len() != Autd3::NUM_TRANSDUCERS {
            return Err(Error::InvalidPayload(
                PayloadError::TransducerCountMismatch {
                    device,
                    got: mask.len(),
                    expected: Autd3::NUM_TRANSDUCERS,
                },
            ));
        }
        for (i, &on) in mask.iter().enumerate() {
            if on {
                out[i / 8] |= 1 << (i % 8);
            }
        }
        Ok(Cmd::SetOutputMask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_mask_packs_bits() {
        let mut mask = vec![false; Autd3::NUM_TRANSDUCERS];
        mask[0] = true;
        mask[3] = true;
        mask[8] = true;
        mask[Autd3::NUM_TRANSDUCERS - 1] = true;
        let data = vec![mask];
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = SetOutputMask { masks: &data }
            .encode(0, 0, &mut out)
            .unwrap();
        assert_eq!(cmd, Cmd::SetOutputMask);
        assert_eq!(out[0], 0b0000_1001);
        assert_eq!(out[1], 0b0000_0001);
        assert_eq!(
            out[(Autd3::NUM_TRANSDUCERS - 1) / 8],
            1 << ((Autd3::NUM_TRANSDUCERS - 1) % 8)
        );
    }

    #[test]
    fn output_mask_rejects_device_out_of_range() {
        let data = vec![vec![true; Autd3::NUM_TRANSDUCERS]];
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            SetOutputMask { masks: &data }.encode(1, 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn output_mask_rejects_wrong_transducer_count() {
        let data = vec![vec![true; Autd3::NUM_TRANSDUCERS + 1]];
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            SetOutputMask { masks: &data }.encode(0, 0, &mut out),
            Err(Error::InvalidPayload(
                PayloadError::TransducerCountMismatch { .. }
            ))
        ));
    }
}
