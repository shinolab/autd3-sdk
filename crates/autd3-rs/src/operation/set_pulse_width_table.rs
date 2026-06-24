use crate::error::Error;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

pub const PWE_TABLE_SIZE: usize = 256;

#[derive(Clone, Copy, Debug)]
pub struct SetPulseWidthTable<'a> {
    pub table: &'a [u16; PWE_TABLE_SIZE],
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
}
