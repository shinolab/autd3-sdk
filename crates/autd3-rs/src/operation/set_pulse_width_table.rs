use crate::error::Error;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::PULSE_WIDTH_PERIOD;

use super::{Distribution, Operation};

pub const PWE_TABLE_SIZE: usize = 256;

#[derive(Clone, Copy, Debug)]
pub struct SetPulseWidthTable<'a> {
    pub table: &'a [u16; PWE_TABLE_SIZE],
}

impl SetPulseWidthTable<'_> {
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn default_table() -> [u16; PWE_TABLE_SIZE] {
        core::array::from_fn(|i| {
            ((i as f32 / 255.0).asin() / core::f32::consts::PI * f32::from(PULSE_WIDTH_PERIOD))
                .round() as u16
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
        for (i, &v) in self.table.iter().enumerate() {
            out[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
        }
        Ok(Cmd::SetPulseWidthTable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pwe_lays_out_le_words() {
        let mut table = [0u16; PWE_TABLE_SIZE];
        for (i, v) in table.iter_mut().enumerate() {
            *v = u16::try_from(i).unwrap();
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
    fn default_table_is_arcsin_shaped() {
        let table = SetPulseWidthTable::default_table();
        assert_eq!(table[0], 0);
        assert_eq!(table[255], 256);
        assert!(table.windows(2).all(|w| w[0] <= w[1]));
        assert!(table.iter().all(|&v| v < PULSE_WIDTH_PERIOD));
    }
}
