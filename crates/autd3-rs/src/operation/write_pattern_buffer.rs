use crate::error::{Error, PayloadError};
use crate::params::{EMISSION_MAX_INDICES, EMISSION_SLOT_WORDS, NUM_TRANSDUCERS};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{Emission, PatternBank};

use super::{Distribution, Operation};

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, little_endian};

#[repr(C)]
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout)]
struct PatternPayload {
    bank: u8,
    _reserved: u8,
    offset: little_endian::U32,
    len: little_endian::U16,
    emissions: [Emission; NUM_TRANSDUCERS],
}

#[derive(Clone, Copy, Debug)]
pub struct WritePatternBuffer<'a> {
    pub bank: PatternBank,
    pub index: usize,
    pub emissions: &'a [[Emission; NUM_TRANSDUCERS]],
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
        device: usize,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        if device >= self.emissions.len() {
            return Err(Error::InvalidPayload(
                PayloadError::EmissionsDeviceOutOfRange {
                    device,
                    len: self.emissions.len(),
                },
            ));
        }
        if self.index >= EMISSION_MAX_INDICES {
            return Err(Error::InvalidPayload(
                PayloadError::PatternIndexOutOfRange {
                    index: self.index,
                    max: EMISSION_MAX_INDICES,
                },
            ));
        }
        let offset =
            u32::try_from(self.index * EMISSION_SLOT_WORDS).expect("bounded by EMISSION_RAM_WORDS");
        let len = u16::try_from(NUM_TRANSDUCERS * 2).expect("fits one frame");
        let (frame, _) =
            PatternPayload::mut_from_prefix(&mut out[..]).expect("PatternPayload fits the payload");
        frame.bank = self.bank.as_u8();
        frame.offset = offset.into();
        frame.len = len.into();
        frame.emissions = self.emissions[device];
        Ok(Cmd::WritePatternBuffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::WRITE_HEADER_BYTES;
    use crate::value::{Intensity, Phase};

    #[test]
    fn write_pattern_lays_out_slot_words() {
        let mut emissions = [Emission::default(); NUM_TRANSDUCERS];
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
        let cmd = op.encode(0, 0, &mut out).unwrap();

        assert_eq!(cmd, Cmd::WritePatternBuffer);
        assert_eq!(out[0], 1);
        let expected_offset = u32::try_from(3 * EMISSION_SLOT_WORDS).unwrap();
        assert_eq!(&out[2..6], &expected_offset.to_le_bytes());
        assert_eq!(&out[6..8], &498u16.to_le_bytes());
        for (i, e) in emissions.iter().enumerate() {
            assert_eq!(out[WRITE_HEADER_BYTES + 2 * i], e.phase.0);
            assert_eq!(out[WRITE_HEADER_BYTES + 2 * i + 1], e.intensity.0);
        }
    }

    #[test]
    fn write_pattern_rejects_index_out_of_range() {
        let patterns = [[Emission::default(); NUM_TRANSDUCERS]];
        let op = WritePatternBuffer {
            bank: PatternBank::B0,
            index: EMISSION_MAX_INDICES,
            emissions: &patterns,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(0, 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn write_pattern_rejects_device_out_of_range() {
        let patterns = [[Emission::default(); NUM_TRANSDUCERS]];
        let op = WritePatternBuffer {
            bank: PatternBank::B0,
            index: 0,
            emissions: &patterns,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(op.encode(0, 0, &mut out).is_ok());
        assert!(matches!(
            op.encode(1, 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }
}
