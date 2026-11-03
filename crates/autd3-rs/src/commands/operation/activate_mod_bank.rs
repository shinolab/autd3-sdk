use autd3_cpu_wire::payload::ActivateModBankPayload;

use crate::datagram::Datagram;
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::Cmd;
use crate::value::{ModulationBank, TransitionMode};

use super::{Operation, encode_header_only};

#[derive(Clone, Copy, Debug)]
pub struct ActivateModulationBank {
    pub bank: ModulationBank,
    pub transition_mode: TransitionMode,
}

impl crate::sealed::Sealed for ActivateModulationBank {}

impl Operation for ActivateModulationBank {
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        encode_header_only(
            out,
            Cmd::ActivateModulationBank,
            &ActivateModBankPayload::new(self.bank, self.transition_mode),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::encode;

    const ENCODED: (Cmd, usize) = (
        Cmd::ActivateModulationBank,
        size_of::<ActivateModBankPayload>(),
    );

    #[test]
    fn activate_mod_bank_lays_out_fields() {
        let (encoded, payload) = encode(&ActivateModulationBank {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::Immediate,
        })
        .unwrap();

        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[0], 1);
        assert_eq!(payload[1], 0xFF);
        assert_eq!(&payload[2..10], &0u64.to_le_bytes());
    }

    #[test]
    fn activate_mod_bank_sys_time_encodes_value() {
        use crate::value::SysTime;

        let (encoded, payload) = encode(&ActivateModulationBank {
            bank: ModulationBank::B0,
            transition_mode: TransitionMode::SysTime {
                time: SysTime::from_nanos(0x0123_4567_89AB_CDEF),
            },
        })
        .unwrap();

        assert_eq!(encoded, ENCODED);
        assert_eq!(payload[1], 0x01);
        assert_eq!(&payload[2..10], &0x0123_4567_89AB_CDEFu64.to_le_bytes());
    }
}
