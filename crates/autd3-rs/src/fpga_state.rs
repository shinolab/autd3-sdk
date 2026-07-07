use autd3_rs_core::value::{ModulationBank, PatternBank};

const BIT_THERMAL_ASSERT: u8 = 0;
const BIT_MOD_BANK: u8 = 1;
const BIT_PATTERN_BANK: u8 = 2;
const BIT_PATTERN_MODE: u8 = 3;
const BIT_READS_ENABLED: u8 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FpgaState(pub u8);

impl FpgaState {
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn is_thermal_asserted(self) -> bool {
        self.0 & (1 << BIT_THERMAL_ASSERT) != 0
    }

    #[must_use]
    pub const fn current_mod_bank(self) -> ModulationBank {
        if self.0 & (1 << BIT_MOD_BANK) != 0 {
            ModulationBank::B1
        } else {
            ModulationBank::B0
        }
    }

    #[must_use]
    pub const fn current_pattern_bank(self) -> PatternBank {
        if self.0 & (1 << BIT_PATTERN_BANK) != 0 {
            PatternBank::B1
        } else {
            PatternBank::B0
        }
    }

    #[must_use]
    pub const fn is_pattern_mode(self) -> bool {
        self.0 & (1 << BIT_PATTERN_MODE) != 0
    }

    #[must_use]
    pub const fn is_stm_mode(self) -> bool {
        !self.is_pattern_mode()
    }

    #[must_use]
    pub const fn reads_enabled(self) -> bool {
        self.0 & (1 << BIT_READS_ENABLED) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_thermal_asserted() {
        assert!(!FpgaState(0b0000_0000).is_thermal_asserted());
        assert!(FpgaState(0b0000_0001).is_thermal_asserted());
    }

    #[test]
    fn current_mod_bank() {
        assert_eq!(
            ModulationBank::B0,
            FpgaState(0b0000_0000).current_mod_bank()
        );
        assert_eq!(
            ModulationBank::B1,
            FpgaState(0b0000_0010).current_mod_bank()
        );
    }

    #[test]
    fn current_pattern_bank() {
        assert_eq!(
            PatternBank::B0,
            FpgaState(0b0000_0000).current_pattern_bank()
        );
        assert_eq!(
            PatternBank::B1,
            FpgaState(0b0000_0100).current_pattern_bank()
        );
    }

    #[test]
    fn pattern_stm_mode() {
        assert!(!FpgaState(0b0000_0000).is_pattern_mode());
        assert!(FpgaState(0b0000_0000).is_stm_mode());
        assert!(FpgaState(0b0000_1000).is_pattern_mode());
        assert!(!FpgaState(0b0000_1000).is_stm_mode());
    }

    #[test]
    fn reads_enabled() {
        assert!(!FpgaState(0b0000_0000).reads_enabled());
        assert!(FpgaState(0b1000_0000).reads_enabled());
    }

    #[test]
    fn raw_roundtrip() {
        assert_eq!(0b1010_1101, FpgaState(0b1010_1101).raw());
    }

    #[test]
    fn decodes_each_bit_independently() {
        let state = FpgaState(0b0000_1110);
        assert!(!state.is_thermal_asserted());
        assert_eq!(ModulationBank::B1, state.current_mod_bank());
        assert_eq!(PatternBank::B1, state.current_pattern_bank());
        assert!(state.is_pattern_mode());
    }
}
