use autd3_cpu_wire::payload::ChangeModBankPayload;
use zerocopy::FromBytes;
use zerocopy::little_endian::{U32, U64};

use crate::error::Error;
use crate::geometry::Device;
use crate::mirror::FirmwareState;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{ModulationBank, TransitionMode};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug)]
pub struct ChangeModulationBank {
    pub bank: ModulationBank,
    pub transition_mode: TransitionMode,
}

impl Operation for ChangeModulationBank {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(
        &self,
        _device: &Device,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        let margin_ns = self.transition_mode.margin_ns()?;
        let (p, _) = ChangeModBankPayload::mut_from_prefix(&mut out[..]).unwrap();
        *p = ChangeModBankPayload {
            bank: self.bank.as_u8(),
            transition_mode: self.transition_mode.as_u8(),
            transition_value: U64::new(self.transition_mode.value()),
            margin_ns: U32::new(margin_ns),
        };
        Ok(Cmd::ChangeModulationBank)
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

    fn encode(op: ChangeModulationBank) -> (Cmd, [u8; PAYLOAD_BYTES]) {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&test_device(0), 0, &mut out).unwrap();
        (cmd, out)
    }

    #[test]
    fn change_mod_bank_lays_out_fields() {
        let (cmd, payload) = encode(ChangeModulationBank {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Immediate,
        });

        assert_eq!(cmd, Cmd::ChangeModulationBank);
        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0xFF);
        assert_eq!(&payload[2..10], &0u64.to_le_bytes());
    }

    #[test]
    fn change_mod_bank_sys_time_encodes_value() {
        use crate::value::DcSysTime;

        let (_cmd, payload) = encode(ChangeModulationBank {
            bank: ModulationBank::B0,
            transition_mode: TransitionMode::SysTime {
                time: DcSysTime::from_nanos(0x0123_4567_89AB_CDEF),
                margin: None,
            },
        });

        assert_eq!(payload[1], 0x01);
        assert_eq!(&payload[2..10], &0x0123_4567_89AB_CDEFu64.to_le_bytes());
    }
}
