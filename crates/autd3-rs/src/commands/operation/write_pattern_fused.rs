use autd3_cpu_wire::fpga_params::NUM_TRANSDUCERS;
use autd3_cpu_wire::layout::PATTERN_RAW_DATA_LEN;
use autd3_cpu_wire::payload::WritePatternFusedPayload;

use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{PatternBank, PatternIntensity, Phase, TransitionMode};

use super::write_pattern_buffer::encode_raw_pattern_entry;
use super::{Distribution, Encoded, Operation, write_header};

#[derive(Clone, Copy, Debug)]
pub(crate) struct WritePatternFused<'a> {
    pub bank: PatternBank,
    pub phases: &'a [Vec<Phase>],
    pub intensities: PatternIntensity<'a>,
    pub transition_mode: TransitionMode,
}

impl crate::sealed::Sealed for WritePatternFused<'_> {}

impl Operation for WritePatternFused<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let header = WritePatternFusedPayload::new(self.bank, self.transition_mode.try_as_wire()?);
        let rest = write_header(out, &header);
        let (dst_phases, dst_intensities) =
            rest[..PATTERN_RAW_DATA_LEN].split_at_mut(NUM_TRANSDUCERS);
        encode_raw_pattern_entry(
            self.phases,
            self.intensities,
            device,
            dst_phases,
            dst_intensities,
        )?;
        Ok(Encoded::header_with_data::<WritePatternFusedPayload>(
            Cmd::WritePatternFused,
            PATTERN_RAW_DATA_LEN,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EncodeError;
    use crate::error::PayloadError;
    use crate::test_utils::{encode, test_device};
    use crate::value::Intensity;

    const HEADER: usize = size_of::<WritePatternFusedPayload>();

    fn op<'a>(
        phases: &'a [Vec<Phase>],
        intensities: impl Into<PatternIntensity<'a>>,
        transition_mode: TransitionMode,
    ) -> WritePatternFused<'a> {
        WritePatternFused {
            bank: PatternBank::B1,
            phases,
            intensities: intensities.into(),
            transition_mode,
        }
    }

    #[test]
    fn the_transition_comes_first_then_phases_then_intensities() {
        let n = test_device(0).num_transducers();
        let phases = [(0..n)
            .map(|i| Phase(u8::try_from(i % 251).unwrap()))
            .collect::<Vec<_>>()];
        let intensities = [(0..n)
            .map(|i| Intensity(u8::try_from((i * 3) % 256).unwrap()))
            .collect::<Vec<_>>()];

        let (cmd, out) = encode(&op(&phases, &intensities, TransitionMode::Ext)).unwrap();

        assert_eq!(
            cmd,
            Encoded::new(Cmd::WritePatternFused, HEADER + PATTERN_RAW_DATA_LEN)
        );
        assert_eq!(out[0], 1, "bank B1");
        assert_eq!(out[1], 0xF0, "EXT");
        assert_eq!(&out[2..10], &0u64.to_le_bytes());
        for i in 0..n {
            assert_eq!(out[HEADER + i], phases[0][i].0);
            assert_eq!(out[HEADER + NUM_TRANSDUCERS + i], intensities[0][i].0);
        }
    }

    #[test]
    fn uniform_intensity_fills_every_transducer() {
        let n = test_device(0).num_transducers();
        let phases = [vec![Phase::ZERO; n]];

        let (_, out) = encode(&op(&phases, Intensity(0x7A), TransitionMode::Immediate)).unwrap();

        assert_eq!(out[1], 0xFF, "IMMEDIATE");
        assert!(
            out[HEADER + NUM_TRANSDUCERS..HEADER + PATTERN_RAW_DATA_LEN]
                .iter()
                .all(|&b| b == 0x7A)
        );
    }

    #[test]
    fn a_missing_device_slot_is_rejected() {
        let phases: [Vec<Phase>; 0] = [];
        assert!(matches!(
            encode(&op(&phases, Intensity::MAX, TransitionMode::Immediate)),
            Err(Error::InvalidPayload(
                PayloadError::DeviceDataOutOfRange { .. }
            ))
        ));
    }

    #[test]
    fn later_is_not_encodable() {
        let n = test_device(0).num_transducers();
        let phases = [vec![Phase::ZERO; n]];
        assert!(matches!(
            encode(&op(&phases, Intensity::MAX, TransitionMode::Later)),
            Err(Error::Encode(EncodeError::TransitionLaterNotEncodable))
        ));
    }
}
