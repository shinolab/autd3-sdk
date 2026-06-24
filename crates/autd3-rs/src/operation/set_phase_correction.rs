use crate::error::{Error, PayloadError};
use crate::params::NUM_TRANSDUCERS;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::Phase;

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug)]
pub struct SetPhaseCorrection<'a> {
    pub phases: &'a [[Phase; NUM_TRANSDUCERS]],
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
        device: usize,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        let phases = self.phases.get(device).ok_or(Error::InvalidPayload(
            PayloadError::EmissionsDeviceOutOfRange {
                device,
                len: self.phases.len(),
            },
        ))?;
        for (i, phase) in phases.iter().enumerate() {
            out[i] = phase.0;
        }
        Ok(Cmd::SetPhaseCorrection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_corr_lays_out_bytes() {
        let mut phases = [Phase::ZERO; NUM_TRANSDUCERS];
        for (i, p) in phases.iter_mut().enumerate() {
            *p = Phase(u8::try_from(i % 256).unwrap());
        }
        let data = [phases];
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = SetPhaseCorrection { phases: &data }
            .encode(0, 0, &mut out)
            .unwrap();
        assert_eq!(cmd, Cmd::SetPhaseCorrection);
        for (i, p) in phases.iter().enumerate() {
            assert_eq!(out[i], p.0);
        }
    }

    #[test]
    fn phase_corr_rejects_device_out_of_range() {
        let data = [[Phase::ZERO; NUM_TRANSDUCERS]];
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            SetPhaseCorrection { phases: &data }.encode(1, 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }
}
