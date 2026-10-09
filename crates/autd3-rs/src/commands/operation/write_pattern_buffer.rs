use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Intensity, PatternBank, PatternIntensity, Phase};

use super::{Distribution, Encoded, Operation, device_slot, write_header};
use autd3_cpu_wire::fpga_params::NUM_TRANSDUCERS;
use autd3_cpu_wire::layout::PATTERN_RAW_DATA_LEN;
use autd3_cpu_wire::payload::WritePatternRawPayload;
use zerocopy::IntoBytes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StmIntensity<'a> {
    Uniform(Intensity),
    Shared(&'a [Vec<Intensity>]),
    PerIndex(&'a [Vec<Vec<Intensity>>]),
}

impl Default for StmIntensity<'_> {
    fn default() -> Self {
        StmIntensity::Uniform(Intensity::MAX)
    }
}

impl<'a> StmIntensity<'a> {
    #[must_use]
    pub(crate) fn at(self, index: usize) -> PatternIntensity<'a> {
        match self {
            StmIntensity::Uniform(intensity) => PatternIntensity::Uniform(intensity),
            StmIntensity::Shared(intensities) => PatternIntensity::PerDevice(intensities),
            StmIntensity::PerIndex(intensities) => PatternIntensity::PerDevice(&intensities[index]),
        }
    }

    pub(crate) const fn per_index_len(self) -> Option<usize> {
        match self {
            StmIntensity::Uniform(_) | StmIntensity::Shared(_) => None,
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
        StmIntensity::Shared(value)
    }
}

impl<'a> From<&'a Vec<Vec<Intensity>>> for StmIntensity<'a> {
    fn from(value: &'a Vec<Vec<Intensity>>) -> Self {
        StmIntensity::Shared(value.as_slice())
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

fn encode_raw_slot(
    phases: &[Vec<Phase>],
    intensities: PatternIntensity<'_>,
    device: &Device,
    dst_phases: &mut [u8],
    dst_intensities: &mut [u8],
) -> Result<(), Error> {
    let phases = device_slot(phases, device)?.as_bytes();
    dst_phases[..phases.len()].copy_from_slice(phases);
    match intensities {
        PatternIntensity::Uniform(intensity) => {
            dst_intensities[..device.num_transducers()].fill(intensity.0);
        }
        PatternIntensity::PerDevice(intensities) => {
            let intensities = device_slot(intensities, device)?.as_bytes();
            dst_intensities[..intensities.len()].copy_from_slice(intensities);
        }
    }
    Ok(())
}

fn encode_raw_frame<'a>(
    bank: PatternBank,
    index: usize,
    slots: impl ExactSizeIterator<Item = (&'a [Vec<Phase>], PatternIntensity<'a>)>,
    device: &Device,
    out: &mut [u8; PAYLOAD_BYTES],
) -> Result<Encoded, Error> {
    let count = slots.len();
    let header = WritePatternRawPayload::new(bank, count, index)?;
    let rest = write_header(out, &header);
    for ((phases, intensities), data) in slots.zip(rest.chunks_mut(PATTERN_RAW_DATA_LEN)) {
        let (dst_phases, dst_intensities) = data.split_at_mut(NUM_TRANSDUCERS);
        encode_raw_slot(phases, intensities, device, dst_phases, dst_intensities)?;
    }
    Ok(Encoded::header_with_data::<WritePatternRawPayload>(
        Cmd::WritePatternRaw,
        count * PATTERN_RAW_DATA_LEN,
    ))
}

impl Operation for WritePatternBuffer<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        encode_raw_frame(
            self.bank,
            self.index,
            core::iter::once((self.phases, self.intensities)),
            device,
            out,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WritePatternBuffers<'a> {
    pub(crate) bank: PatternBank,
    pub(crate) index: usize,
    pub(crate) phases: &'a [Vec<Vec<Phase>>],
    pub(crate) intensities: StmIntensity<'a>,
}

impl crate::sealed::Sealed for WritePatternBuffers<'_> {}

impl Operation for WritePatternBuffers<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        encode_raw_frame(
            self.bank,
            self.index,
            self.phases
                .iter()
                .enumerate()
                .map(|(k, phases)| (phases.as_slice(), self.intensities.at(self.index + k))),
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
    const PHASES_OFFSET: usize = size_of::<WritePatternRawPayload>();
    const INTENSITIES_OFFSET: usize = size_of::<WritePatternRawPayload>() + NUM_TRANSDUCERS;

    #[test]
    fn write_pattern_lays_out_phases_then_intensities() {
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
            assert_eq!(out[PHASES_OFFSET + i], phases[0][i].0);
            assert_eq!(out[INTENSITIES_OFFSET + i], intensities[0][i].0);
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
        let op = WritePatternBuffers {
            bank: PatternBank::B0,
            index: 10,
            phases: &phases,
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
        let op = WritePatternBuffers {
            bank: PatternBank::B0,
            index: EMISSION_MAX_INDICES - 1,
            phases: &phases,
            intensities: StmIntensity::default(),
        };
        assert!(matches!(
            encode(&op),
            Err(Error::InvalidPayload(
                PayloadError::PatternIndexOutOfRange { .. }
            ))
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
