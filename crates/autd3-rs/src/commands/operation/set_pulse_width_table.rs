use autd3_cpu_wire::cpu_params::PWE_DEFAULT_TABLE;
use autd3_cpu_wire::payload::PwePayload;
use zerocopy::little_endian::U16;

use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::PulseWidth;

use super::{Encoded, Operation, encode_fixed};

pub use autd3_cpu_wire::layout::PWE_TABLE_SIZE;

#[derive(Clone, Copy, Debug)]
pub struct SetPulseWidthTable<'a> {
    pub table: &'a [PulseWidth; PWE_TABLE_SIZE],
}

static DEFAULT_TABLE: [PulseWidth; PWE_TABLE_SIZE] = {
    let mut table = [PulseWidth::new(0); PWE_TABLE_SIZE];
    let mut i = 0;
    while i < PWE_TABLE_SIZE {
        table[i] = PulseWidth::new(PWE_DEFAULT_TABLE[i]);
        i += 1;
    }
    table
};

impl SetPulseWidthTable<'_> {
    #[must_use]
    pub const fn empty_table() -> [PulseWidth; PWE_TABLE_SIZE] {
        [PulseWidth::new(0); PWE_TABLE_SIZE]
    }
}

impl Default for SetPulseWidthTable<'static> {
    fn default() -> Self {
        Self {
            table: &DEFAULT_TABLE,
        }
    }
}

impl crate::sealed::Sealed for SetPulseWidthTable<'_> {}

impl Operation for SetPulseWidthTable<'_> {
    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let mut table = [U16::ZERO; PWE_TABLE_SIZE];
        for (dst, &v) in table.iter_mut().zip(self.table.iter()) {
            dst.set(v.pulse_width()?);
        }
        Ok(encode_fixed(
            out,
            Cmd::SetPulseWidthTable,
            &PwePayload { table },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::encode;
    use crate::value::PULSE_WIDTH_PERIOD;

    #[test]
    fn pwe_lays_out_le_words() {
        let mut table = SetPulseWidthTable::empty_table();
        for (i, v) in table.iter_mut().enumerate() {
            *v = PulseWidth::new(u16::try_from(i).unwrap());
        }
        let (cmd, out) = encode(&SetPulseWidthTable { table: &table }).unwrap();
        assert_eq!(
            cmd,
            Encoded::new(Cmd::SetPulseWidthTable, size_of::<PwePayload>())
        );
        assert_eq!(&out[0..2], &0u16.to_le_bytes());
        assert_eq!(&out[2..4], &1u16.to_le_bytes());
        assert_eq!(&out[510..512], &255u16.to_le_bytes());
    }

    #[test]
    fn pwe_rejects_out_of_range() {
        let mut table = SetPulseWidthTable::empty_table();
        table[0] = PulseWidth::new(PULSE_WIDTH_PERIOD);
        assert!(matches!(
            encode(&SetPulseWidthTable { table: &table }),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn default_is_arcsin_shaped() {
        let table = SetPulseWidthTable::default().table;
        for (i, v) in table.iter().enumerate() {
            let expected = ((i as f32 / 255.0).asin() / core::f32::consts::PI
                * f32::from(PULSE_WIDTH_PERIOD))
            .round() as u16;
            assert_eq!(v.pulse_width(), Ok(expected));
        }
    }

    #[test]
    fn default_encodes_the_boot_table() {
        let (_, out) = encode(&SetPulseWidthTable::default()).unwrap();
        for (bytes, v) in out.as_chunks::<2>().0.iter().zip(PWE_DEFAULT_TABLE) {
            assert_eq!(*bytes, v.to_le_bytes());
        }
    }

    #[test]
    fn empty_table_is_all_zero() {
        assert!(
            SetPulseWidthTable::empty_table()
                .iter()
                .all(|v| v.pulse_width() == Ok(0))
        );
    }
}
