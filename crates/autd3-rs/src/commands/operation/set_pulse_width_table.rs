use autd3_cpu_wire::payload::PwePayload;
use zerocopy::FromBytes;
use zerocopy::little_endian::U16;

use crate::error::Error;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{PULSE_WIDTH_PERIOD, PulseWidth};

use super::{Distribution, Operation};

pub use autd3_cpu_wire::layout::PWE_TABLE_SIZE;

#[derive(Clone, Copy, Debug)]
pub struct SetPulseWidthTable<'a> {
    pub table: &'a [PulseWidth; PWE_TABLE_SIZE],
}

impl SetPulseWidthTable<'_> {
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn default_table() -> [PulseWidth; PWE_TABLE_SIZE] {
        core::array::from_fn(|i| {
            PulseWidth::new(
                ((i as f32 / 255.0).asin() / core::f32::consts::PI * f32::from(PULSE_WIDTH_PERIOD))
                    .round() as u16,
            )
        })
    }
}

impl Operation for SetPulseWidthTable<'_> {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(
        &self,
        _device: usize,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        let (p, _) = PwePayload::mut_from_prefix(&mut out[..]).unwrap();
        for (dst, &v) in p.table.iter_mut().zip(self.table.iter()) {
            let pulse_width = v.pulse_width()?;
            *dst = U16::new(pulse_width);
        }
        Ok(Cmd::SetPulseWidthTable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pwe_lays_out_le_words() {
        let mut table = [PulseWidth::new(0); PWE_TABLE_SIZE];
        for (i, v) in table.iter_mut().enumerate() {
            *v = PulseWidth::new(u16::try_from(i).unwrap());
        }
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = SetPulseWidthTable { table: &table }
            .encode(0, 0, &mut out)
            .unwrap();
        assert_eq!(cmd, Cmd::SetPulseWidthTable);
        assert_eq!(&out[0..2], &0u16.to_le_bytes());
        assert_eq!(&out[2..4], &1u16.to_le_bytes());
        assert_eq!(&out[510..512], &255u16.to_le_bytes());
    }

    #[test]
    fn pwe_rejects_out_of_range() {
        let mut table = [PulseWidth::new(0); PWE_TABLE_SIZE];
        table[0] = PulseWidth::new(PULSE_WIDTH_PERIOD);
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            SetPulseWidthTable { table: &table }.encode(0, 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }

    #[test]
    fn default_table_is_arcsin_shaped() {
        let table = SetPulseWidthTable::default_table();
        assert_eq!(table[0].pulse_width(), Ok(0));
        assert_eq!(table[255].pulse_width(), Ok(256));
        assert!(table.windows(2).all(|w| w[0] <= w[1]));
        assert!(
            table
                .iter()
                .all(|&v| v.pulse_width().unwrap() < PULSE_WIDTH_PERIOD)
        );
    }
}
