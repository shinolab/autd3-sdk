use zerocopy::little_endian::U64;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{TransitionMode, try_read_exact};
use crate::value::Transition;
use crate::{Error, ModulationBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ActivateModBankPayload {
    pub bank: ModulationBank,
    pub transition_mode: TransitionMode,
    pub transition_value: U64,
}

impl ActivateModBankPayload {
    #[must_use]
    pub const fn new(bank: ModulationBank, transition: Transition) -> Self {
        Self {
            bank,
            transition_mode: transition.mode(),
            transition_value: U64::new(transition.value()),
        }
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

const _: () = assert!(core::mem::offset_of!(ActivateModBankPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(ActivateModBankPayload, transition_mode) == 1);
const _: () = assert!(core::mem::offset_of!(ActivateModBankPayload, transition_value) == 2);
const _: () = assert!(core::mem::size_of::<ActivateModBankPayload>() == 10);

#[cfg(test)]
mod tests {
    use zerocopy::IntoBytes;

    use super::*;
    use crate::value::{GpioIn, SysTime};

    #[test]
    fn a_built_payload_passes_parse() {
        let built = ActivateModBankPayload::new(
            ModulationBank::B1,
            Transition::SysTime(SysTime::from_nanos(0x0123_4567_89AB)),
        );
        let parsed = ActivateModBankPayload::parse(built.as_bytes()).unwrap();
        assert_eq!(parsed.bank, ModulationBank::B1);
        assert_eq!(parsed.transition_mode, TransitionMode::SysTime);
        assert_eq!(parsed.transition_value.get(), 0x0123_4567_89AB);
    }

    #[test]
    fn a_gpio_transition_carries_the_pin_number() {
        let built = ActivateModBankPayload::new(ModulationBank::B0, Transition::Gpio(GpioIn::I2));
        assert_eq!(built.transition_mode, TransitionMode::Gpio);
        assert_eq!(built.transition_value.get(), 2);
    }
}
