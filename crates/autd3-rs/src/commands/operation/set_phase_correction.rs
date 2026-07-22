use autd3_cpu_wire::payload::PhaseCorrPayload;
use zerocopy::FromBytes;

use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::Phase;

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug)]
pub struct SetPhaseCorrection<'a> {
    pub phases: &'a [Vec<Phase>],
}

impl Operation for SetPhaseCorrection<'_> {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(
        &self,
        device: &Device,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        let phases =
            self.phases
                .get(device.idx())
                .ok_or(PayloadError::EmissionsDeviceOutOfRange {
                    device: device.idx(),
                    len: self.phases.len(),
                })?;
        if phases.len() != device.num_transducers() {
            return Err(PayloadError::TransducerCountMismatch {
                device: device.idx(),
                got: phases.len(),
                expected: device.num_transducers(),
            }
            .into());
        }
        let (p, _) = PhaseCorrPayload::mut_from_prefix(&mut out[..]).unwrap();
        p.data
            .iter_mut()
            .zip(phases)
            .for_each(|(dst, phase)| *dst = phase.0);
        Ok(Cmd::SetPhaseCorrection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;

    #[test]
    fn phase_corr_lays_out_bytes() {
        let dev = test_device(0);
        let phases: Vec<Phase> = (0..dev.num_transducers())
            .map(|i| Phase(u8::try_from(i % 256).unwrap()))
            .collect();
        let data = vec![phases.clone()];
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = SetPhaseCorrection { phases: &data }
            .encode(&dev, 0, &mut out)
            .unwrap();
        assert_eq!(cmd, Cmd::SetPhaseCorrection);
        for (i, p) in phases.iter().enumerate() {
            assert_eq!(out[i], p.0);
        }
    }

    #[test]
    fn phase_corr_rejects_device_out_of_range() {
        let dev = test_device(1);
        let data = vec![vec![Phase::ZERO; dev.num_transducers()]];
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            SetPhaseCorrection { phases: &data }.encode(&dev, 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn phase_corr_rejects_wrong_transducer_count() {
        let dev = test_device(0);
        let data = vec![vec![Phase::ZERO; dev.num_transducers() - 1]];
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            SetPhaseCorrection { phases: &data }.encode(&dev, 0, &mut out),
            Err(Error::InvalidPayload(
                PayloadError::TransducerCountMismatch { .. }
            ))
        ));
    }
}
