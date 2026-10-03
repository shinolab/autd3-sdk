use autd3_cpu_wire::payload::ActivatePatternBankPayload;
use zerocopy::little_endian::{U32, U64};

use crate::error::Error;
use crate::geometry::Device;
use crate::mirror::FirmwareState;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{PatternBank, TransitionMode};

use super::{Distribution, Encoded, Operation, write_header};

#[derive(Clone, Copy, Debug)]
pub struct ActivatePatternBank {
    pub bank: PatternBank,
    pub transition_mode: TransitionMode,
}

impl crate::sealed::Sealed for ActivatePatternBank {}

impl Operation for ActivatePatternBank {
    fn apply_clock_offset(&mut self, offset_ns: i64) {
        self.transition_mode = self.transition_mode.with_clock_offset(offset_ns);
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let margin_ns = self.transition_mode.margin_ns()?;
        write_header(
            out,
            &ActivatePatternBankPayload {
                bank: self.bank,
                transition_mode: self.transition_mode.try_as_wire()?,
                transition_value: U64::new(self.transition_mode.value()),
                margin_ns: U32::new(margin_ns),
            },
        );
        Ok(Encoded::header::<ActivatePatternBankPayload>(
            Cmd::ActivatePatternBank,
        ))
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        let bank = self.bank.as_u8();
        state.silencer.check_pattern_bank(device, bank)?;
        state
            .transition
            .check_pattern_bank(device, bank, self.transition_mode)?;
        state.silencer.note_pattern_bank(bank);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;

    fn encode(op: ActivatePatternBank) -> (Cmd, [u8; PAYLOAD_BYTES]) {
        let mut out = [0u8; PAYLOAD_BYTES];
        let encoded = op.encode(&test_device(0), &mut out).unwrap();
        assert_eq!(encoded.len, size_of::<ActivatePatternBankPayload>());
        (encoded.cmd, out)
    }

    #[test]
    fn activate_pattern_bank_lays_out_fields() {
        let (cmd, payload) = encode(ActivatePatternBank {
            bank: PatternBank::B1,
            transition_mode: TransitionMode::Immediate,
        });

        assert_eq!(cmd, Cmd::ActivatePatternBank);
        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0xFF);
        assert_eq!(&payload[2..10], &0u64.to_le_bytes());
    }

    #[test]
    fn activate_pattern_bank_encodes_transition_value() {
        use crate::value::SysTime;

        let (_cmd, payload) = encode(ActivatePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::SysTime {
                time: SysTime::from_nanos(0x0123_4567_89AB_CDEF),
                margin: None,
            },
        });

        assert_eq!(payload[0], 0);
        assert_eq!(payload[1], 0x01);
        assert_eq!(&payload[2..10], &0x0123_4567_89AB_CDEFu64.to_le_bytes());
    }

    #[test]
    fn activate_pattern_bank_encodes_gpio_pin() {
        use crate::value::GpioIn;

        let (_cmd, payload) = encode(ActivatePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::Gpio(GpioIn::I2),
        });

        assert_eq!(payload[1], 0x02);
        assert_eq!(&payload[2..10], &2u64.to_le_bytes());
    }

    #[test]
    fn activate_pattern_bank_encodes_sys_time_margin() {
        use core::time::Duration;

        use crate::value::SysTime;

        let (_cmd, payload) = encode(ActivatePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::SysTime {
                time: SysTime::ZERO,
                margin: Some(Duration::from_millis(1)),
            },
        });
        assert_eq!(&payload[10..14], &1_000_000u32.to_le_bytes());

        let (_cmd, payload) = encode(ActivatePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::SysTime {
                time: SysTime::ZERO,
                margin: None,
            },
        });
        assert_eq!(&payload[10..14], &0u32.to_le_bytes());
    }

    #[test]
    fn activate_pattern_bank_rejects_margin_beyond_u32_nanos() {
        use core::time::Duration;

        use crate::value::SysTime;

        let mut out = [0u8; PAYLOAD_BYTES];
        let err = ActivatePatternBank {
            bank: PatternBank::B0,
            transition_mode: TransitionMode::SysTime {
                time: SysTime::ZERO,
                margin: Some(Duration::from_secs(5)),
            },
        }
        .encode(&test_device(0), &mut out)
        .unwrap_err();

        assert!(matches!(
            err,
            Error::Encode(crate::EncodeError::TransitionMarginOutOfRange(_))
        ));
    }

    #[test]
    fn activate_pattern_bank_refuses_to_not_transition() {
        let mut out = [0u8; PAYLOAD_BYTES];
        let err = ActivatePatternBank {
            bank: PatternBank::B1,
            transition_mode: TransitionMode::Later,
        }
        .encode(&test_device(0), &mut out)
        .unwrap_err();

        assert!(matches!(
            err,
            Error::Encode(crate::EncodeError::TransitionLaterNotEncodable)
        ));
    }
}
