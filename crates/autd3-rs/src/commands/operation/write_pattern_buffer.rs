use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::params::EMISSION_MAX_INDICES;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Intensity, PatternBank, Phase};

use super::{Distribution, Operation};
use autd3_cpu_wire::layout::{PATTERN_RAW_DATA_LEN, PATTERN_RAW_MAX_COUNT};
use autd3_cpu_wire::params::NUM_TRANSDUCERS;
use autd3_cpu_wire::payload::WritePatternRawPayload;
use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, IntoBytes};

const RAW_HEADER_BYTES: usize = core::mem::size_of::<WritePatternRawPayload>();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternIntensity<'a> {
    Uniform(Intensity),
    PerDevice(&'a [Vec<Intensity>]),
}

impl Default for PatternIntensity<'_> {
    fn default() -> Self {
        PatternIntensity::Uniform(Intensity::MAX)
    }
}

impl From<Intensity> for PatternIntensity<'_> {
    fn from(value: Intensity) -> Self {
        PatternIntensity::Uniform(value)
    }
}

impl<'a> From<&'a [Vec<Intensity>]> for PatternIntensity<'a> {
    fn from(value: &'a [Vec<Intensity>]) -> Self {
        PatternIntensity::PerDevice(value)
    }
}

impl<'a> From<&'a Vec<Vec<Intensity>>> for PatternIntensity<'a> {
    fn from(value: &'a Vec<Vec<Intensity>>) -> Self {
        PatternIntensity::PerDevice(value.as_slice())
    }
}

impl<'a, const N: usize> From<&'a [Vec<Intensity>; N]> for PatternIntensity<'a> {
    fn from(value: &'a [Vec<Intensity>; N]) -> Self {
        PatternIntensity::PerDevice(value.as_slice())
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

fn device_slot<'a, T>(data: &'a [Vec<T>], device: &Device) -> Result<&'a [T], Error> {
    let slot = data
        .get(device.idx())
        .ok_or(PayloadError::DeviceDataOutOfRange {
            device: device.idx(),
            len: data.len(),
        })?;
    if slot.len() != device.num_transducers() {
        return Err(PayloadError::TransducerCountMismatch {
            device: device.idx(),
            got: slot.len(),
            expected: device.num_transducers(),
        }
        .into());
    }
    Ok(slot)
}

pub(crate) fn device_phases<'a>(
    phases: &'a [Vec<Phase>],
    device: &Device,
) -> Result<&'a [Phase], Error> {
    device_slot(phases, device)
}

pub(crate) fn encode_raw_slot(
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

fn encode_raw_frame(
    bank: PatternBank,
    index: usize,
    slots: &[(&[Vec<Phase>], PatternIntensity<'_>)],
    device: &Device,
    out: &mut [u8; PAYLOAD_BYTES],
) -> Result<Cmd, Error> {
    let last = index + slots.len().max(1) - 1;
    if last >= EMISSION_MAX_INDICES {
        return Err(PayloadError::PatternIndexOutOfRange {
            index: last,
            max: EMISSION_MAX_INDICES,
        }
        .into());
    }
    let (p, rest) = WritePatternRawPayload::mut_from_prefix(&mut out[..]).unwrap();
    p.bank = bank.as_u8();
    p.count = u8::try_from(slots.len()).expect("at most PATTERN_RAW_MAX_COUNT slots");
    p.index = U16::new(u16::try_from(index).expect("bounded by EMISSION_MAX_INDICES"));
    for (&(phases, intensities), data) in slots.iter().zip(rest.chunks_mut(PATTERN_RAW_DATA_LEN)) {
        let (dst_phases, dst_intensities) = data.split_at_mut(NUM_TRANSDUCERS);
        encode_raw_slot(phases, intensities, device, dst_phases, dst_intensities)?;
    }
    Ok(Cmd::WritePatternRaw)
}

impl Operation for WritePatternBuffer<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        encode_raw_frame(
            self.bank,
            self.index,
            &[(self.phases, self.intensities)],
            device,
            out,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WritePatternBuffers<'a> {
    pub(crate) bank: PatternBank,
    pub(crate) index: usize,
    pub(crate) count: usize,
    pub(crate) slots: [(&'a [Vec<Phase>], PatternIntensity<'a>); PATTERN_RAW_MAX_COUNT],
}

impl crate::sealed::Sealed for WritePatternBuffers<'_> {}

impl Operation for WritePatternBuffers<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        encode_raw_frame(
            self.bank,
            self.index,
            &self.slots[..self.count.clamp(1, PATTERN_RAW_MAX_COUNT)],
            device,
            out,
        )
    }
}

const _: () =
    assert!(RAW_HEADER_BYTES + PATTERN_RAW_MAX_COUNT * PATTERN_RAW_DATA_LEN <= PAYLOAD_BYTES);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;
    const PHASES_OFFSET: usize = RAW_HEADER_BYTES;
    const INTENSITIES_OFFSET: usize = RAW_HEADER_BYTES + NUM_TRANSDUCERS;

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

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&dev, &mut out).unwrap();

        assert_eq!(cmd, Cmd::WritePatternRaw);
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

        let mut uniform = [0u8; PAYLOAD_BYTES];
        WritePatternBuffer::new(PatternBank::B1, 3, &phases, Intensity(0x7A))
            .encode(&dev, &mut uniform)
            .unwrap();

        let mut per_device = [0u8; PAYLOAD_BYTES];
        WritePatternBuffer::new(PatternBank::B1, 3, &phases, &intensities)
            .encode(&dev, &mut per_device)
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
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&dev, &mut out),
            Err(Error::InvalidPayload(_))
        ));
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
        let first = [vec![Phase(1); n]];
        let second = [vec![Phase(2); n]];
        let op = WritePatternBuffers {
            bank: PatternBank::B0,
            index: 10,
            count: 2,
            slots: [
                (&first[..], PatternIntensity::Uniform(Intensity(0x11))),
                (&second[..], PatternIntensity::Uniform(Intensity(0x22))),
            ],
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert_eq!(op.encode(&dev, &mut out).unwrap(), Cmd::WritePatternRaw);
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
        let phases = [vec![Phase::ZERO; dev.num_transducers()]];
        let slot = (&phases[..], PatternIntensity::default());
        let op = WritePatternBuffers {
            bank: PatternBank::B0,
            index: EMISSION_MAX_INDICES - 1,
            count: 2,
            slots: [slot, slot],
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&dev, &mut out),
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
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&dev, &mut out),
            Err(Error::InvalidPayload(
                PayloadError::TransducerCountMismatch { .. }
            ))
        ));
    }
}
