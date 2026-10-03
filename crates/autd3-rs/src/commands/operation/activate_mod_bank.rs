use autd3_cpu_wire::payload::ActivateModBankPayload;
use zerocopy::little_endian::{U32, U64};

use crate::error::Error;
use crate::geometry::Device;
use crate::mirror::FirmwareState;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{ModulationBank, TransitionMode};

use super::{Distribution, Encoded, Operation, write_header};

#[derive(Clone, Copy, Debug)]
pub struct ActivateModulationBank {
    pub bank: ModulationBank,
    pub transition_mode: TransitionMode,
}

impl crate::sealed::Sealed for ActivateModulationBank {}

impl Operation for ActivateModulationBank {
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
            &ActivateModBankPayload {
                bank: self.bank,
                transition_mode: self.transition_mode.try_as_wire()?,
                transition_value: U64::new(self.transition_mode.value()),
                margin_ns: U32::new(margin_ns),
            },
        );
        Ok(Encoded::header::<ActivateModBankPayload>(
            Cmd::ActivateModulationBank,
        ))
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        let bank = self.bank.as_u8();
        state.silencer.check_mod_bank(device, bank)?;
        state
            .transition
            .check_mod_bank(device, bank, self.transition_mode)?;
        state.silencer.note_mod_bank(bank);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;

    fn encode(op: ActivateModulationBank) -> (Cmd, [u8; PAYLOAD_BYTES]) {
        let mut out = [0u8; PAYLOAD_BYTES];
        let encoded = op.encode(&test_device(0), &mut out).unwrap();
        assert_eq!(encoded.len, size_of::<ActivateModBankPayload>());
        (encoded.cmd, out)
    }

    #[test]
    fn activate_mod_bank_lays_out_fields() {
        let (cmd, payload) = encode(ActivateModulationBank {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Immediate,
        });

        assert_eq!(cmd, Cmd::ActivateModulationBank);
        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0xFF);
        assert_eq!(&payload[2..10], &0u64.to_le_bytes());
    }

    #[test]
    fn activate_mod_bank_sys_time_encodes_value() {
        use crate::value::SysTime;

        let (_cmd, payload) = encode(ActivateModulationBank {
            bank: ModulationBank::B0,
            transition_mode: TransitionMode::SysTime {
                time: SysTime::from_nanos(0x0123_4567_89AB_CDEF),
                margin: None,
            },
        });

        assert_eq!(payload[1], 0x01);
        assert_eq!(&payload[2..10], &0x0123_4567_89AB_CDEFu64.to_le_bytes());
    }

    #[test]
    fn activate_mod_bank_refuses_to_not_transition() {
        use autd3_rs_core::error::EncodeError;

        let mut out = [0u8; PAYLOAD_BYTES];
        let e = ActivateModulationBank {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Later,
        }
        .encode(&test_device(0), &mut out)
        .unwrap_err();
        assert!(matches!(
            e,
            Error::Encode(EncodeError::TransitionLaterNotEncodable)
        ));
    }
}
