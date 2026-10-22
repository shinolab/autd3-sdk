use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Intensity, PatternBank, PatternIntensity, Phase};

use super::{Distribution, Encoded, Operation, checked_device_row, write_header};
use autd3_cpu_wire::layout::PATTERN_RAW_DATA_LEN;
use autd3_cpu_wire::payload::WritePatternRawPayload;
use zerocopy::IntoBytes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StmIntensity<'a> {
    Uniform(Intensity),
    SameForAllIndices(&'a [Vec<Intensity>]),
    PerIndex(&'a [Vec<Vec<Intensity>>]),
}

impl Default for StmIntensity<'_> {
    fn default() -> Self {
        StmIntensity::Uniform(Intensity::MAX)
    }
}

impl<'a> StmIntensity<'a> {
    #[must_use]
    pub(crate) fn for_pattern(self, index: usize) -> PatternIntensity<'a> {
        match self {
            StmIntensity::Uniform(intensity) => PatternIntensity::Uniform(intensity),
            StmIntensity::SameForAllIndices(intensities) => {
                PatternIntensity::PerDevice(intensities)
            }
            StmIntensity::PerIndex(intensities) => PatternIntensity::PerDevice(&intensities[index]),
        }
    }

    pub(crate) const fn per_index_len(self) -> Option<usize> {
        match self {
            StmIntensity::Uniform(_) | StmIntensity::SameForAllIndices(_) => None,
            StmIntensity::PerIndex(intensities) => Some(intensities.len()),
        }
    }
}

impl From<Intensity> for StmIntensity<'_> {
    fn from(value: Intensity) -> Self {
        StmIntensity::Uniform(value)
    }
}

impl<'a> From<&'a [Vec<Intensity>]> for StmIntensity<'a> {
    fn from(value: &'a [Vec<Intensity>]) -> Self {
        StmIntensity::SameForAllIndices(value)
    }
}

impl<'a> From<&'a Vec<Vec<Intensity>>> for StmIntensity<'a> {
    fn from(value: &'a Vec<Vec<Intensity>>) -> Self {
        StmIntensity::SameForAllIndices(value.as_slice())
    }
}

impl<'a> From<&'a [Vec<Vec<Intensity>>]> for StmIntensity<'a> {
    fn from(value: &'a [Vec<Vec<Intensity>>]) -> Self {
        StmIntensity::PerIndex(value)
    }
}

impl<'a> From<&'a Vec<Vec<Vec<Intensity>>>> for StmIntensity<'a> {
    fn from(value: &'a Vec<Vec<Vec<Intensity>>>) -> Self {
        StmIntensity::PerIndex(value.as_slice())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WritePatternBuffer<'a> {
    pub bank: PatternBank,
    pub index: usize,
    pub phases: &'a [Vec<Phase>],
    pub intensities: PatternIntensity<'a>,
}

impl<'a> WritePatternBuffer<'a> {
    #[must_use]
    pub fn new(
        bank: PatternBank,
        index: usize,
        phases: &'a [Vec<Phase>],
        intensities: impl Into<PatternIntensity<'a>>,
    ) -> Self {
        Self {
            bank,
            index,
            phases,
            intensities: intensities.into(),
        }
    }
}

impl crate::sealed::Sealed for WritePatternBuffer<'_> {}

pub(crate) fn encode_raw_pattern_entry(
    phases: &[Vec<Phase>],
    intensities: PatternIntensity<'_>,
    device: &Device,
    dst: &mut [u8; PATTERN_RAW_DATA_LEN],
) -> Result<(), Error> {
    let phases = checked_device_row(phases, device)?.as_bytes();
    let (dst, _) = dst.as_chunks_mut::<2>();
    match intensities {
        PatternIntensity::Uniform(intensity) => {
            for (pair, &phase) in dst.iter_mut().zip(phases) {
                *pair = [phase, intensity.0];
            }
        }
        PatternIntensity::PerDevice(intensities) => {
            let intensities = checked_device_row(intensities, device)?.as_bytes();
            for (pair, (&phase, &intensity)) in dst.iter_mut().zip(phases.iter().zip(intensities)) {
                *pair = [phase, intensity];
            }
        }
    }
    Ok(())
}

fn encode_raw_patterns<'a>(
    bank: PatternBank,
    start_index: usize,
    patterns: impl ExactSizeIterator<Item = (&'a [Vec<Phase>], PatternIntensity<'a>)>,
    device: &Device,
    out: &mut [u8; PAYLOAD_BYTES],
) -> Result<Encoded, Error> {
    let num_patterns = patterns.len();
    let header = WritePatternRawPayload::new(bank, num_patterns, start_index)?;
    let rest = write_header(out, &header);
    let (slots, _) = rest.as_chunks_mut::<PATTERN_RAW_DATA_LEN>();
    for ((phases, intensities), data) in patterns.zip(slots) {
        encode_raw_pattern_entry(phases, intensities, device, data)?;
    }
    Ok(Encoded::header_with_data::<WritePatternRawPayload>(
        Cmd::WritePatternRaw,
        num_patterns * PATTERN_RAW_DATA_LEN,
    ))
}

impl Operation for WritePatternBuffer<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        encode_raw_patterns(
            self.bank,
            self.index,
            core::iter::once((self.phases, self.intensities)),
            device,
            out,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WritePatternChunk<'a> {
    pub(crate) bank: PatternBank,
    pub(crate) start_index: usize,
    pub(crate) patterns: &'a [Vec<Vec<Phase>>],
    pub(crate) intensities: StmIntensity<'a>,
}

impl crate::sealed::Sealed for WritePatternChunk<'_> {}

impl Operation for WritePatternChunk<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        encode_raw_patterns(
            self.bank,
            self.start_index,
            self.patterns.iter().enumerate().map(|(k, phases)| {
                (
                    phases.as_slice(),
                    self.intensities.for_pattern(self.start_index + k),
                )
            }),
            device,
            out,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PayloadError;
    use crate::params::EMISSION_MAX_INDICES;
    use crate::test_utils::{encode, test_device};
    use autd3_cpu_wire::payload::PayloadBuildError;
    const PHASES_OFFSET: usize = size_of::<WritePatternRawPayload>();
    const INTENSITIES_OFFSET: usize = size_of::<WritePatternRawPayload>() + 1;

    #[test]
    fn write_pattern_interleaves_phase_and_intensity_per_transducer() {
        let dev = test_device(0);
        let n = dev.num_transducers();
        let phases = [(0..n)
            .map(|i| Phase(u8::try_from(i % 251).unwrap()))
            .collect::<Vec<_>>()];
        let intensities = [(0..n)
            .map(|i| Intensity(u8::try_from((i * 3) % 256).unwrap()))
            .collect::<Vec<_>>()];
        let op = WritePatternBuffer::new(PatternBank::B1, 3, &phases, &intensities);

        let (cmd, out) = encode(&op).unwrap();

        assert_eq!(
            cmd,
            Encoded::header_with_data::<WritePatternRawPayload>(
                Cmd::WritePatternRaw,
                PATTERN_RAW_DATA_LEN
            )
        );
        assert_eq!(out[0], 1);
        assert_eq!(out[1], 1, "count");
        assert_eq!(&out[2..4], &3u16.to_le_bytes());
        for i in 0..n {
            assert_eq!(out[PHASES_OFFSET + 2 * i], phases[0][i].0);
            assert_eq!(out[INTENSITIES_OFFSET + 2 * i], intensities[0][i].0);
        }
    }

    #[test]
    fn uniform_intensity_matches_an_explicit_buffer() {
        let dev = test_device(0);
        let n = dev.num_transducers();
        let phases = [(0..n)
            .map(|i| Phase(u8::try_from(i % 251).unwrap()))
            .collect::<Vec<_>>()];
        let intensities = [vec![Intensity(0x7A); n]];

        let uniform = encode(&WritePatternBuffer::new(
            PatternBank::B1,
            3,
            &phases,
            Intensity(0x7A),
        ))
        .unwrap();
        let per_device = encode(&WritePatternBuffer::new(
            PatternBank::B1,
            3,
            &phases,
            &intensities,
        ))
        .unwrap();

        assert_eq!(uniform, per_device);
    }

    #[test]
    fn uniform_intensity_does_not_need_a_device_slot() {
        let dev = test_device(1);
        let phases = vec![vec![Phase::ZERO; dev.num_transducers()]; 2];
        let op = WritePatternBuffer::new(PatternBank::B0, 0, &phases, PatternIntensity::default());
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(op.encode(&dev, &mut out).is_ok());
        assert_eq!(out[INTENSITIES_OFFSET], Intensity::MAX.0);
    }

    #[test]
    fn write_pattern_rejects_index_out_of_range() {
        let dev = test_device(0);
        let phases = [vec![Phase::ZERO; dev.num_transducers()]];
        let intensities = [vec![Intensity::MAX; dev.num_transducers()]];
        let op =
            WritePatternBuffer::new(PatternBank::B0, EMISSION_MAX_INDICES, &phases, &intensities);
        assert!(matches!(encode(&op), Err(Error::InvalidPayload(_))));
    }

    #[test]
    fn write_pattern_rejects_device_out_of_range() {
        let dev = test_device(0);
        let phases = [vec![Phase::ZERO; dev.num_transducers()]];
        let intensities = [vec![Intensity::MAX; dev.num_transducers()]];
        let op = WritePatternBuffer::new(PatternBank::B0, 0, &phases, &intensities);
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(op.encode(&dev, &mut out).is_ok());
        assert!(matches!(
            op.encode(&test_device(1), &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn two_indices_share_one_frame() {
        let dev = test_device(0);
        let n = dev.num_transducers();
        let phases = [vec![vec![Phase(1); n]], vec![vec![Phase(2); n]]];
        let mut intensities = vec![vec![vec![Intensity::MIN; n]]; 10];
        intensities.push(vec![vec![Intensity(0x11); n]]);
        intensities.push(vec![vec![Intensity(0x22); n]]);
        let op = WritePatternChunk {
            bank: PatternBank::B0,
            start_index: 10,
            patterns: &phases,
            intensities: StmIntensity::PerIndex(&intensities),
        };
        let (encoded, out) = encode(&op).unwrap();
        assert_eq!(
            encoded,
            Encoded::header_with_data::<WritePatternRawPayload>(
                Cmd::WritePatternRaw,
                2 * PATTERN_RAW_DATA_LEN
            )
        );
        assert_eq!(out[1], 2, "count");
        assert_eq!(&out[2..4], &10u16.to_le_bytes());
        assert_eq!(out[PHASES_OFFSET], 1);
        assert_eq!(out[INTENSITIES_OFFSET], 0x11);
        assert_eq!(out[PHASES_OFFSET + PATTERN_RAW_DATA_LEN], 2);
        assert_eq!(out[INTENSITIES_OFFSET + PATTERN_RAW_DATA_LEN], 0x22);
    }

    #[test]
    fn two_indices_must_fit_below_the_last_slot() {
        let dev = test_device(0);
        let phases = vec![vec![vec![Phase::ZERO; dev.num_transducers()]]; 2];
        let op = WritePatternChunk {
            bank: PatternBank::B0,
            start_index: EMISSION_MAX_INDICES - 1,
            patterns: &phases,
            intensities: StmIntensity::default(),
        };
        assert!(matches!(
            encode(&op),
            Err(Error::InvalidPayload(PayloadError::Build(
                PayloadBuildError::PatternIndexOutOfRange { .. }
            )))
        ));
    }

    #[test]
    fn write_pattern_rejects_mismatched_intensity_length() {
        let dev = test_device(0);
        let phases = [vec![Phase::ZERO; dev.num_transducers()]];
        let intensities = [vec![Intensity::MAX; dev.num_transducers() - 1]];
        let op = WritePatternBuffer::new(PatternBank::B0, 0, &phases, &intensities);
        assert!(matches!(
            encode(&op),
            Err(Error::InvalidPayload(
                PayloadError::TransducerCountMismatch { .. }
            ))
        ));
    }
}
