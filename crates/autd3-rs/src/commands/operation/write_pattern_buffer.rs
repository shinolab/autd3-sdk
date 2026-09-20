use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::params::EMISSION_MAX_INDICES;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Intensity, PatternBank, Phase};

use super::{Distribution, Operation};
use autd3_cpu_wire::layout::PATTERN_RAW_DATA_LEN;
use autd3_cpu_wire::payload::WritePatternRawPayload;
use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, IntoBytes};

#[derive(Clone, Copy, Debug)]
pub struct WritePatternBuffer<'a> {
    pub bank: PatternBank,
    pub index: usize,
    pub phases: &'a [Vec<Phase>],
    pub intensities: &'a [Vec<Intensity>],
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
    intensities: &[Vec<Intensity>],
    device: &Device,
    dst: &mut [u8],
) -> Result<(), Error> {
    let phases = device_slot(phases, device)?.as_bytes();
    let intensities = device_slot(intensities, device)?.as_bytes();
    let (p, i) = dst[..PATTERN_RAW_DATA_LEN].split_at_mut(PATTERN_RAW_DATA_LEN / 2);
    p[..phases.len()].copy_from_slice(phases);
    i[..intensities.len()].copy_from_slice(intensities);
    Ok(())
}

impl Operation for WritePatternBuffer<'_> {
    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        if self.index >= EMISSION_MAX_INDICES {
            return Err(PayloadError::PatternIndexOutOfRange {
                index: self.index,
                max: EMISSION_MAX_INDICES,
            }
            .into());
        }
        let (h, rest) = WritePatternRawPayload::mut_from_prefix(&mut out[..]).unwrap();
        encode_raw_slot(self.phases, self.intensities, device, rest)?;
        *h = WritePatternRawPayload {
            bank: self.bank.as_u8(),
            reserved: 0,
            index: U16::new(u16::try_from(self.index).expect("bounded by EMISSION_MAX_INDICES")),
        };
        Ok(Cmd::WritePatternRaw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;
    const HEADER_BYTES: usize = core::mem::size_of::<WritePatternRawPayload>();

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
        let op = WritePatternBuffer {
            bank: PatternBank::B1,
            index: 3,
            phases: &phases,
            intensities: &intensities,
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&dev, &mut out).unwrap();

        assert_eq!(cmd, Cmd::WritePatternRaw);
        assert_eq!(out[0], 1);
        assert_eq!(&out[2..4], &3u16.to_le_bytes());
        for i in 0..n {
            assert_eq!(out[HEADER_BYTES + i], phases[0][i].0);
            assert_eq!(out[HEADER_BYTES + n + i], intensities[0][i].0);
        }
    }

    #[test]
    fn write_pattern_rejects_index_out_of_range() {
        let dev = test_device(0);
        let phases = [vec![Phase::ZERO; dev.num_transducers()]];
        let intensities = [vec![Intensity::MAX; dev.num_transducers()]];
        let op = WritePatternBuffer {
            bank: PatternBank::B0,
            index: EMISSION_MAX_INDICES,
            phases: &phases,
            intensities: &intensities,
        };
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
        let op = WritePatternBuffer {
            bank: PatternBank::B0,
            index: 0,
            phases: &phases,
            intensities: &intensities,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(op.encode(&dev, &mut out).is_ok());
        assert!(matches!(
            op.encode(&test_device(1), &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn write_pattern_rejects_mismatched_intensity_length() {
        let dev = test_device(0);
        let phases = [vec![Phase::ZERO; dev.num_transducers()]];
        let intensities = [vec![Intensity::MAX; dev.num_transducers() - 1]];
        let op = WritePatternBuffer {
            bank: PatternBank::B0,
            index: 0,
            phases: &phases,
            intensities: &intensities,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&dev, &mut out),
            Err(Error::InvalidPayload(
                PayloadError::TransducerCountMismatch { .. }
            ))
        ));
    }
}
