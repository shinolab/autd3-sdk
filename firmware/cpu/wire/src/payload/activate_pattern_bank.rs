use zerocopy::little_endian::U64;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{TransitionModeKind, try_read_exact};
use crate::value::TransitionMode;
use crate::{Error, PatternBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ActivatePatternBankPayload {
    pub bank: PatternBank,
    pub transition_mode: TransitionModeKind,
    pub transition_value: U64,
}

impl ActivatePatternBankPayload {
    #[must_use]
    pub const fn new(bank: PatternBank, transition: TransitionMode) -> Self {
        Self {
            bank,
            transition_mode: transition.kind(),
            transition_value: U64::new(transition.value()),
        }
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

assert_layout!(ActivatePatternBankPayload {
    bank: 0,
    transition_mode: 1,
    transition_value: 2,
} size 10);

#[cfg(test)]
mod tests {
    use zerocopy::IntoBytes;

    use super::*;
    use crate::value::{GpioIn, SysTime};

    #[test]
    fn a_built_payload_passes_parse() {
        let built = ActivatePatternBankPayload::new(
            PatternBank::B1,
            TransitionMode::SysTime {
                time: SysTime::from_nanos(0x0123_4567_89AB),
            },
        );
        let parsed = ActivatePatternBankPayload::parse(built.as_bytes()).unwrap();
        assert_eq!(parsed.bank, PatternBank::B1);
        assert_eq!(parsed.transition_mode, TransitionModeKind::SysTime);
        assert_eq!(parsed.transition_value.get(), 0x0123_4567_89AB);
    }

    #[test]
    fn a_gpio_transition_carries_the_pin_number() {
        let built =
            ActivatePatternBankPayload::new(PatternBank::B0, TransitionMode::Gpio(GpioIn::I2));
        assert_eq!(built.transition_mode, TransitionModeKind::Gpio);
        assert_eq!(built.transition_value.get(), 2);
    }
}
