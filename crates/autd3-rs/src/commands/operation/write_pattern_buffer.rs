use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::params::{EMISSION_MAX_INDICES, EMISSION_SLOT_WORDS};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Emission, PatternBank};

use super::{Distribution, Operation};
use autd3_cpu_wire::payload::WritePatternPayload;
use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, IntoBytes};

#[derive(Clone, Copy, Debug)]
pub struct WritePatternBuffer<'a> {
    pub bank: PatternBank,
    pub index: usize,
    pub emissions: &'a [Vec<Emission>],
}

impl Operation for WritePatternBuffer<'_> {
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
        if device.idx() >= self.emissions.len() {
            return Err(PayloadError::EmissionsDeviceOutOfRange {
                device: device.idx(),
                len: self.emissions.len(),
            }
            .into());
        }
        if self.index >= EMISSION_MAX_INDICES {
            return Err(PayloadError::PatternIndexOutOfRange {
                index: self.index,
                max: EMISSION_MAX_INDICES,
            }
            .into());
        }
        let emissions = &self.emissions[device.idx()];
        if emissions.len() != device.num_transducers() {
            return Err(PayloadError::TransducerCountMismatch {
                device: device.idx(),
                got: emissions.len(),
                expected: device.num_transducers(),
            }
            .into());
        }
        let offset =
            u32::try_from(self.index * EMISSION_SLOT_WORDS).expect("bounded by EMISSION_RAM_WORDS");
        let bytes = emissions.as_bytes();
        let (h, rest) = WritePatternPayload::mut_from_prefix(&mut out[..]).unwrap();
        if bytes.len() > rest.len() {
            return Err(PayloadError::PatternWriteExceedsCapacity {
                device: device.idx(),
                len: bytes.len(),
                capacity: rest.len(),
            }
            .into());
        }
        let len = u16::try_from(bytes.len()).expect("bounded by frame capacity");
        *h = WritePatternPayload {
            bank: self.bank.as_u8(),
            reserved: 0,
            offset: U32::new(offset),
            data_len: U16::new(len),
        };
        rest[..bytes.len()].copy_from_slice(bytes);
        Ok(Cmd::WritePatternBuffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;
    use crate::value::{Intensity, Phase};
    use autd3_cpu_wire::layout::WRITE_HEADER_BYTES;

    #[test]
    fn write_pattern_lays_out_slot_words() {
        let dev = test_device(0);
        let mut emissions = vec![Emission::default(); dev.num_transducers()];
        for (i, e) in emissions.iter_mut().enumerate() {
            e.phase = Phase(u8::try_from(i % 251).unwrap());
            e.intensity = Intensity(u8::try_from((i * 3) % 256).unwrap());
        }
        let patterns = [emissions];
        let op = WritePatternBuffer {
            bank: PatternBank::B1,
            index: 3,
            emissions: &patterns,
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&dev, 0, &mut out).unwrap();

        assert_eq!(cmd, Cmd::WritePatternBuffer);
        assert_eq!(out[0], 1);
        let expected_offset = u32::try_from(3 * EMISSION_SLOT_WORDS).unwrap();
        assert_eq!(&out[2..6], &expected_offset.to_le_bytes());
        assert_eq!(
            &out[6..8],
            &u16::try_from(dev.num_transducers() * 2)
                .unwrap()
                .to_le_bytes()
        );
        for (i, e) in patterns[0].iter().enumerate() {
            assert_eq!(out[WRITE_HEADER_BYTES + 2 * i], e.phase.0);
            assert_eq!(out[WRITE_HEADER_BYTES + 2 * i + 1], e.intensity.0);
        }
    }

    #[test]
    fn write_pattern_rejects_index_out_of_range() {
        let dev = test_device(0);
        let patterns = [vec![Emission::default(); dev.num_transducers()]];
        let op = WritePatternBuffer {
            bank: PatternBank::B0,
            index: EMISSION_MAX_INDICES,
            emissions: &patterns,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&dev, 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn write_pattern_rejects_device_out_of_range() {
        let dev = test_device(0);
        let patterns = [vec![Emission::default(); dev.num_transducers()]];
        let op = WritePatternBuffer {
            bank: PatternBank::B0,
            index: 0,
            emissions: &patterns,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(op.encode(&dev, 0, &mut out).is_ok());
        assert!(matches!(
            op.encode(&test_device(1), 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }
}
