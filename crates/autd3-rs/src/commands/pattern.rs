use autd3_cpu_wire::layout::PATTERN_RAW_DATA_LEN;
use autd3_cpu_wire::payload::WritePatternFusedPayload;

use crate::commands::operation::{
    Distribution, Encoded, Operation, PatternIntensity, encode_raw_pattern_entry, write_header,
};
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{PatternBank, Phase, TransitionMode};

#[derive(Clone, Copy, Debug)]
pub struct Pattern<'a> {
    pub bank: PatternBank,
    pub phases: &'a [Vec<Phase>],
    pub intensities: PatternIntensity<'a>,
    pub transition_mode: TransitionMode,
}

impl<'a> Pattern<'a> {
    #[must_use]
    pub fn new(phases: &'a [Vec<Phase>], intensities: impl Into<PatternIntensity<'a>>) -> Self {
        Self::with_bank(PatternBank::B0, phases, intensities)
    }

    #[must_use]
    pub fn with_bank(
        bank: PatternBank,
        phases: &'a [Vec<Phase>],
        intensities: impl Into<PatternIntensity<'a>>,
    ) -> Self {
        Self {
            bank,
            phases,
            intensities: intensities.into(),
            transition_mode: TransitionMode::Immediate,
        }
    }
}

impl crate::sealed::Sealed for Pattern<'_> {}

impl Operation for Pattern<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let header = WritePatternFusedPayload::new(self.bank, self.transition_mode.as_wire());
        let rest = write_header(out, &header);
        let (data, _) = rest
            .split_first_chunk_mut::<PATTERN_RAW_DATA_LEN>()
            .expect("the pattern fits in the payload");
        encode_raw_pattern_entry(self.phases, self.intensities, device, data)?;
        Ok(Encoded::header_with_data::<WritePatternFusedPayload>(
            Cmd::WritePatternFused,
            PATTERN_RAW_DATA_LEN,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::operation::WritePatternBuffer;
    use crate::datagram::Frames;
    use crate::error::PayloadError;
    use crate::geometry::Autd3;
    use crate::test_utils::{build, cmds, encode, payload, test_device, test_geometry};
    use crate::value::Intensity;
    use autd3_cpu_wire::payload::WritePatternRawPayload;

    #[test]
    fn pattern_expands_to_a_single_fused_frame() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 2];
        let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]; 2];
        let datagrams = build(2, Pattern::new(&phases, &intensities)).unwrap();

        assert_eq!(cmds(&datagrams), [Cmd::WritePatternFused]);
        for device in 0..2 {
            let fused = payload(&datagrams, 0, device);
            assert_eq!(
                fused.len(),
                size_of::<WritePatternFusedPayload>() + 2 * Autd3::NUM_TRANSDUCERS
            );
            assert_eq!(fused[0], 0, "bank B0");
            assert_eq!(fused[1], 0xFF, "IMMEDIATE");
        }
    }

    #[test]
    fn the_fused_frame_carries_the_same_data_as_the_write_frame() {
        let phases: Vec<Vec<Phase>> = (0..2)
            .map(|d| {
                (0..Autd3::NUM_TRANSDUCERS)
                    .map(|i| Phase(u8::try_from((i + d) % 256).unwrap()))
                    .collect()
            })
            .collect();
        let intensities: Vec<Vec<Intensity>> = (0..2)
            .map(|d| {
                (0..Autd3::NUM_TRANSDUCERS)
                    .map(|i| Intensity(u8::try_from((3 * i + d) % 256).unwrap()))
                    .collect()
            })
            .collect();
        let fused = build(
            2,
            Pattern::with_bank(PatternBank::B1, &phases, &intensities),
        )
        .unwrap();
        let raw = build(
            2,
            WritePatternBuffer::new(PatternBank::B1, 0, &phases, &intensities),
        )
        .unwrap();

        for device in 0..2 {
            assert_eq!(payload(&fused, 0, device)[0], 1, "bank B1");
            assert_eq!(
                &payload(&fused, 0, device)[size_of::<WritePatternFusedPayload>()..],
                &payload(&raw, 0, device)[size_of::<WritePatternRawPayload>()..]
            );
        }
    }

    #[test]
    fn the_transition_comes_first_then_phase_and_intensity_per_transducer() {
        let n = test_device(0).num_transducers();
        let phases = [(0..n)
            .map(|i| Phase(u8::try_from(i % 251).unwrap()))
            .collect::<Vec<_>>()];
        let intensities = [(0..n)
            .map(|i| Intensity(u8::try_from((i * 3) % 256).unwrap()))
            .collect::<Vec<_>>()];

        let (cmd, out) = encode(&Pattern {
            transition_mode: TransitionMode::Ext,
            ..Pattern::with_bank(PatternBank::B1, &phases, &intensities)
        })
        .unwrap();

        assert_eq!(
            cmd,
            Encoded::new(
                Cmd::WritePatternFused,
                size_of::<WritePatternFusedPayload>() + PATTERN_RAW_DATA_LEN
            )
        );
        assert_eq!(out[0], 1, "bank B1");
        assert_eq!(out[1], 0xF0, "EXT");
        assert_eq!(&out[2..10], &0u64.to_le_bytes());
        for i in 0..n {
            assert_eq!(
                out[size_of::<WritePatternFusedPayload>() + 2 * i],
                phases[0][i].0
            );
            assert_eq!(
                out[size_of::<WritePatternFusedPayload>() + 2 * i + 1],
                intensities[0][i].0
            );
        }
    }

    #[test]
    fn uniform_intensity_fills_every_transducer() {
        let n = test_device(0).num_transducers();
        let phases = [vec![Phase::ZERO; n]];

        let (_, out) = encode(&Pattern::new(&phases, Intensity(0x7A))).unwrap();

        assert_eq!(out[1], 0xFF, "IMMEDIATE");
        assert!(
            out[size_of::<WritePatternFusedPayload>()
                ..size_of::<WritePatternFusedPayload>() + PATTERN_RAW_DATA_LEN]
                .iter()
                .skip(1)
                .step_by(2)
                .all(|&b| b == 0x7A)
        );
    }

    #[test]
    fn too_few_device_rows_fail_the_encoding_without_leaving_a_frame() {
        let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]; 1];
        let mut frames = Frames::default();
        let result = frames.encode_into(&test_geometry(2), Pattern::new(&phases, Intensity::MAX));

        assert!(matches!(
            result,
            Err(Error::InvalidPayload(PayloadError::DeviceDataOutOfRange {
                device: 1,
                len: 1
            }))
        ));
        assert!(frames.is_empty());
    }

    #[test]
    fn a_missing_device_slot_is_rejected() {
        let phases: [Vec<Phase>; 0] = [];
        assert!(matches!(
            encode(&Pattern::new(&phases, Intensity::MAX)),
            Err(Error::InvalidPayload(
                PayloadError::DeviceDataOutOfRange { .. }
            ))
        ));
    }
}
