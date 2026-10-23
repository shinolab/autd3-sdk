use autd3_cpu_wire::payload::ActivatePatternBankPayload;

use crate::datagram::Datagram;
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::Cmd;
use crate::value::{PatternBank, TransitionMode};

use super::{Operation, encode_header_only};

#[derive(Clone, Copy, Debug)]
pub struct ActivatePatternBank {
    pub bank: PatternBank,
    pub transition_mode: TransitionMode,
}

impl crate::sealed::Sealed for ActivatePatternBank {}

impl Operation for ActivatePatternBank {
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        encode_header_only(
            out,
            Cmd::ActivatePatternBank,
            &ActivatePatternBankPayload::new(self.bank, self.transition_mode),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::encode;

    const ENCODED: (Cmd, usize) = (
        Cmd::ActivatePatternBank,
        size_of::<ActivatePatternBankPayload>(),
    );

    #[test]
    fn activate_pattern_bank_lays_out_fields() {
        let (encoded, payload) = encode(&ActivatePatternBank {
            bank: PatternBank::B1,
            transition_mode: TransitionMode::Immediate,
        })
        .unwrap();

        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0xFF);
        assert_eq!(&payload[2..10], &0u64.to_le_bytes());
    }

    #[test]
    fn activate_pattern_bank_encodes_transition_value() {
        use crate::value::SysTime;

        let (encoded, payload) = encode(&ActivatePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::SysTime {
                time: SysTime::from_nanos(0x0123_4567_89AB_CDEF),
            },
        })
        .unwrap();

        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[0], 0);
        assert_eq!(payload[1], 0x01);
        assert_eq!(&payload[2..10], &0x0123_4567_89AB_CDEFu64.to_le_bytes());
    }

    #[test]
    fn activate_pattern_bank_encodes_gpio_pin() {
        use crate::value::GpioIn;

        let (encoded, payload) = encode(&ActivatePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::Gpio(GpioIn::I2),
        })
        .unwrap();

        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[1], 0x02);
        assert_eq!(&payload[2..10], &2u64.to_le_bytes());
    }
}
