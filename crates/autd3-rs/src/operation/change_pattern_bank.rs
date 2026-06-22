use crate::error::Error;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{PatternBank, TransitionMode};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug)]
pub struct ChangePatternBank {
    pub bank: PatternBank,
    pub transition_mode: TransitionMode,
    pub transition_value: u64,
}

impl Operation for ChangePatternBank {
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
        out[0] = self.bank.as_u8();
        out[1] = self.transition_mode.as_u8();
        out[2..10].copy_from_slice(&self.transition_value.to_le_bytes());
        Ok(Cmd::ChangePatternBank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(op: ChangePatternBank) -> (Cmd, [u8; PAYLOAD_BYTES]) {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(0, 0, &mut out).unwrap();
        (cmd, out)
    }

    #[test]
    fn change_pattern_bank_lays_out_fields() {
        let (cmd, payload) = encode(ChangePatternBank {
            bank: PatternBank::B1,
            transition_mode: TransitionMode::Immediate,
            transition_value: 0,
        });

        assert_eq!(cmd, Cmd::ChangePatternBank);
        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0xFF);
        assert_eq!(&payload[2..10], &0u64.to_le_bytes());
    }

    #[test]
    fn change_pattern_bank_encodes_transition_value() {
        let (_cmd, payload) = encode(ChangePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::SysTime,
            transition_value: 0x0123_4567_89AB_CDEF,
        });

        assert_eq!(payload[0], 0);
        assert_eq!(payload[1], 0x01);
        assert_eq!(&payload[2..10], &0x0123_4567_89AB_CDEFu64.to_le_bytes());
    }
}
