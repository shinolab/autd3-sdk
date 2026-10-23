use crate::datagram::Datagram;
use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::protocol::Cmd;
use crate::value::{Intensity, PatternBank, PatternIntensity, Phase};

use super::{Distribution, Operation, checked_device_row, set_cmd, write_header};
use autd3_cpu_wire::layout::{PATTERN_RAW_DATA_LEN, PATTERN_RAW_MAX_COUNT};
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

impl<'a, const N: usize> From<&'a [Vec<Intensity>; N]> for StmIntensity<'a> {
    fn from(value: &'a [Vec<Intensity>; N]) -> Self {
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

impl<'a, const N: usize> From<&'a [Vec<Vec<Intensity>>; N]> for StmIntensity<'a> {
    fn from(value: &'a [Vec<Vec<Intensity>>; N]) -> Self {
        StmIntensity::PerIndex(value.as_slice())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WritePatternBuffer<'a> {
    pub bank: PatternBank,
    pub start_index: usize,
    pub patterns: &'a [Vec<Vec<Phase>>],
    pub intensities: StmIntensity<'a>,
}

impl<'a> WritePatternBuffer<'a> {
    pub const MAX_LEN: usize = PATTERN_RAW_MAX_COUNT;

    #[must_use]
    pub fn new(
        bank: PatternBank,
        start_index: usize,
        patterns: &'a [Vec<Vec<Phase>>],
        intensities: impl Into<StmIntensity<'a>>,
    ) -> Self {
        Self {
            bank,
            start_index,
            patterns,
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

impl Operation for WritePatternBuffer<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut Datagram) -> Result<(), Error> {
        let num_patterns = self.patterns.len();
        let header = WritePatternRawPayload::new(self.bank, num_patterns, self.start_index)?;
        let required = self.start_index + num_patterns;
        if let Some(len) = self.intensities.per_index_len()
            && len < required
        {
            return Err(PayloadError::PerIndexIntensityTooShort { required, len }.into());
        }
        let rest = write_header(out, &header);
        let (slots, _) = rest.as_chunks_mut::<PATTERN_RAW_DATA_LEN>();
        for (k, (phases, data)) in self.patterns.iter().zip(slots).enumerate() {
            encode_raw_pattern_entry(
                phases,
                self.intensities.for_pattern(self.start_index + k),
                device,
                data,
            )?;
        }
        set_cmd(
            out,
            Cmd::WritePatternRaw,
            size_of::<WritePatternRawPayload>() + num_patterns * PATTERN_RAW_DATA_LEN,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::EMISSION_MAX_INDICES;
    use crate::test_utils::{encode, test_device};
    use autd3_cpu_wire::payload::PayloadBuildError;
    const PHASES_OFFSET: usize = size_of::<WritePatternRawPayload>();
    const INTENSITIES_OFFSET: usize = size_of::<WritePatternRawPayload>() + 1;

    #[test]
    fn write_pattern_interleaves_phase_and_intensity_per_transducer() {
        let dev = test_device(0);
        let n = dev.num_transducers();
        let patterns = [vec![
            (0..n)
                .map(|i| Phase(u8::try_from(i % 251).unwrap()))
                .collect::<Vec<_>>(),
        ]];
        let intensities = [(0..n)
            .map(|i| Intensity(u8::try_from((i * 3) % 256).unwrap()))
            .collect::<Vec<_>>()];
        let op = WritePatternBuffer::new(PatternBank::B1, 3, &patterns, &intensities);

        let (cmd, out) = encode(&op).unwrap();

        assert_eq!(
            cmd,
            (
                Cmd::WritePatternRaw,
                size_of::<WritePatternRawPayload>() + PATTERN_RAW_DATA_LEN
            )
        );
        assert_eq!(out[0], 1);
        assert_eq!(out[1], 1, "count");
        assert_eq!(&out[2..4], &3u16.to_le_bytes());
        for i in 0..n {
            assert_eq!(out[PHASES_OFFSET + 2 * i], patterns[0][0][i].0);
            assert_eq!(out[INTENSITIES_OFFSET + 2 * i], intensities[0][i].0);
        }
    }

    #[test]
    fn uniform_intensity_matches_an_explicit_buffer() {
        let dev = test_device(0);
        let n = dev.num_transducers();
        let patterns = [vec![
            (0..n)
                .map(|i| Phase(u8::try_from(i % 251).unwrap()))
                .collect::<Vec<_>>(),
        ]];
        let intensities = [vec![Intensity(0x7A); n]];

        let uniform = encode(&WritePatternBuffer::new(
            PatternBank::B1,
            3,
            &patterns,
            Intensity(0x7A),
        ))
        .unwrap();
        let per_device = encode(&WritePatternBuffer::new(
            PatternBank::B1,
            3,
            &patterns,
            &intensities,
        ))
        .unwrap();

        assert_eq!(uniform, per_device);
    }

    #[test]
    fn uniform_intensity_does_not_need_a_device_slot() {
        let dev = test_device(1);
        let patterns = [vec![vec![Phase::ZERO; dev.num_transducers()]; 2]];
        let op = WritePatternBuffer::new(PatternBank::B0, 0, &patterns, StmIntensity::default());
        let mut out = Datagram::no_payload(Cmd::Nop);
        assert!(op.encode(&dev, &mut out).is_ok());
        assert_eq!(out.payload[INTENSITIES_OFFSET], Intensity::MAX.0);
    }

    #[test]
    fn write_pattern_rejects_index_out_of_range() {
        let dev = test_device(0);
        let patterns = [vec![vec![Phase::ZERO; dev.num_transducers()]]];
        let op = WritePatternBuffer::new(
            PatternBank::B0,
            EMISSION_MAX_INDICES,
            &patterns,
            Intensity::MAX,
        );
        assert!(matches!(encode(&op), Err(Error::InvalidPayload(_))));
    }

    #[test]
    fn write_pattern_rejects_device_out_of_range() {
        let dev = test_device(0);
        let patterns = [vec![vec![Phase::ZERO; dev.num_transducers()]]];
        let intensities = [vec![Intensity::MAX; dev.num_transducers()]];
        let op = WritePatternBuffer::new(PatternBank::B0, 0, &patterns, &intensities);
        let mut out = Datagram::no_payload(Cmd::Nop);
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
        let patterns = [vec![vec![Phase(1); n]], vec![vec![Phase(2); n]]];
        let mut intensities = vec![vec![vec![Intensity::MIN; n]]; 10];
        intensities.push(vec![vec![Intensity(0x11); n]]);
        intensities.push(vec![vec![Intensity(0x22); n]]);
        let op = WritePatternBuffer::new(PatternBank::B0, 10, &patterns, &intensities);
        let (encoded, out) = encode(&op).unwrap();
        assert_eq!(
            encoded,
            (
                Cmd::WritePatternRaw,
                size_of::<WritePatternRawPayload>() + 2 * PATTERN_RAW_DATA_LEN
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
        let patterns = vec![vec![vec![Phase::ZERO; dev.num_transducers()]]; 2];
        let op = WritePatternBuffer::new(
            PatternBank::B0,
            EMISSION_MAX_INDICES - 1,
            &patterns,
            StmIntensity::default(),
        );
        assert!(matches!(
            encode(&op),
            Err(Error::InvalidPayload(PayloadError::Build(
                PayloadBuildError::PatternIndexOutOfRange { .. }
            )))
        ));
    }

    #[test]
    fn more_patterns_than_one_frame_holds_are_rejected() {
        let dev = test_device(0);
        let patterns =
            vec![vec![vec![Phase::ZERO; dev.num_transducers()]]; WritePatternBuffer::MAX_LEN + 1];
        let op = WritePatternBuffer::new(PatternBank::B0, 0, &patterns, Intensity::MAX);
        assert!(matches!(
            encode(&op),
            Err(Error::InvalidPayload(PayloadError::Build(
                PayloadBuildError::PatternCountExceedsFrame { count, max }
            ))) if count == WritePatternBuffer::MAX_LEN + 1 && max == WritePatternBuffer::MAX_LEN
        ));
    }

    #[test]
    fn per_index_intensities_must_cover_every_written_index() {
        let dev = test_device(0);
        let n = dev.num_transducers();
        let patterns = vec![vec![vec![Phase::ZERO; n]]; 2];
        let intensities = vec![vec![vec![Intensity::MAX; n]]; 3];
        assert!(
            encode(&WritePatternBuffer::new(
                PatternBank::B0,
                1,
                &patterns,
                &intensities
            ))
            .is_ok()
        );
        assert!(matches!(
            encode(&WritePatternBuffer::new(
                PatternBank::B0,
                2,
                &patterns,
                &intensities
            )),
            Err(Error::InvalidPayload(
                PayloadError::PerIndexIntensityTooShort {
                    required: 4,
                    len: 3
                }
            ))
        ));
    }

    #[test]
    fn write_pattern_rejects_mismatched_intensity_length() {
        let dev = test_device(0);
        let patterns = [vec![vec![Phase::ZERO; dev.num_transducers()]]];
        let intensities = [vec![Intensity::MAX; dev.num_transducers() - 1]];
        let op = WritePatternBuffer::new(PatternBank::B0, 0, &patterns, &intensities);
        assert!(matches!(
            encode(&op),
            Err(Error::InvalidPayload(
                PayloadError::TransducerCountMismatch { .. }
            ))
        ));
    }
}
