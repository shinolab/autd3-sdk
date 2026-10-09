use autd3_cpu_wire::fpga_params::NUM_TRANSDUCERS;
use autd3_cpu_wire::payload::PhaseCorrPayload;
use zerocopy::IntoBytes;

use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::Phase;

use super::{Distribution, Encoded, Operation, device_slot, encode_fixed};

#[derive(Clone, Copy, Debug)]
pub struct SetPhaseCorrection<'a> {
    pub phases: &'a [Vec<Phase>],
}

impl crate::sealed::Sealed for SetPhaseCorrection<'_> {}

impl Operation for SetPhaseCorrection<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let phases = device_slot(self.phases, device)?.as_bytes();
        let mut data = [0u8; NUM_TRANSDUCERS];
        data[..phases.len()].copy_from_slice(phases);
        Ok(encode_fixed(
            out,
            Cmd::SetPhaseCorrection,
            &PhaseCorrPayload { data },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PayloadError;
    use crate::test_utils::{encode, test_device};

    #[test]
    fn phase_corr_lays_out_bytes() {
        let dev = test_device(0);
        let phases: Vec<Phase> = (0..dev.num_transducers())
            .map(|i| Phase(u8::try_from(i % 256).unwrap()))
            .collect();
        let data = vec![phases.clone()];
        let (cmd, out) = encode(&SetPhaseCorrection { phases: &data }).unwrap();
        assert_eq!(
            cmd,
            Encoded::new(Cmd::SetPhaseCorrection, size_of::<PhaseCorrPayload>())
        );
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
            SetPhaseCorrection { phases: &data }.encode(&dev, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn phase_corr_rejects_wrong_transducer_count() {
        let dev = test_device(0);
        let data = vec![vec![Phase::ZERO; dev.num_transducers() - 1]];
        assert!(matches!(
            encode(&SetPhaseCorrection { phases: &data }),
            Err(Error::InvalidPayload(
                PayloadError::TransducerCountMismatch { .. }
            ))
        ));
    }
}
