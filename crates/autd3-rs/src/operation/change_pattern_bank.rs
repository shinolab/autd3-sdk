use crate::error::Error;
use crate::mirror::FirmwareState;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{PatternBank, TransitionMode};

use super::{Distribution, Operation, silencer_constraint};

#[derive(Clone, Copy, Debug)]
pub struct ChangePatternBank {
    pub bank: PatternBank,
    pub transition_mode: TransitionMode,
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
        out[2..10].copy_from_slice(&self.transition_mode.value().to_le_bytes());
        Ok(Cmd::ChangePatternBank)
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        let bank = self.bank.as_u8();
        if let Err(v) = state.silencer.check_pattern_bank(bank) {
            return Err(silencer_constraint(device, v));
        }
        state.silencer.note_pattern_bank(bank);
        Ok(())
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
        });

        assert_eq!(cmd, Cmd::ChangePatternBank);
        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0xFF);
        assert_eq!(&payload[2..10], &0u64.to_le_bytes());
    }

    #[test]
    fn change_pattern_bank_encodes_transition_value() {
        use crate::value::DcSysTime;

        let (_cmd, payload) = encode(ChangePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::SysTime(DcSysTime::from_nanos(0x0123_4567_89AB_CDEF)),
        });

        assert_eq!(payload[0], 0);
        assert_eq!(payload[1], 0x01);
        assert_eq!(&payload[2..10], &0x0123_4567_89AB_CDEFu64.to_le_bytes());
    }

    #[test]
    fn change_pattern_bank_encodes_gpio_pin() {
        use crate::value::GpioIn;

        let (_cmd, payload) = encode(ChangePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::Gpio(GpioIn::I2),
        });

        assert_eq!(payload[1], 0x02);
        assert_eq!(&payload[2..10], &2u64.to_le_bytes());
    }
}
