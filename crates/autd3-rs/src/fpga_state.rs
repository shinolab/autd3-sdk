use autd3_cpu_wire::fpga_params::FpgaStateFlags;
use autd3_rs_core::value::{ModulationBank, PatternBank};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FpgaState(pub u8);

impl FpgaState {
    const fn flags(self) -> FpgaStateFlags {
        FpgaStateFlags::from_bits_retain(self.0)
    }

    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn is_thermal_asserted(self) -> bool {
        self.flags().contains(FpgaStateFlags::THERMAL_ASSERT)
    }

    #[must_use]
    pub const fn current_mod_bank(self) -> ModulationBank {
        if self.flags().contains(FpgaStateFlags::MOD_BANK) {
            ModulationBank::B1
        } else {
            ModulationBank::B0
        }
    }

    #[must_use]
    pub const fn current_pattern_bank(self) -> PatternBank {
        if self.flags().contains(FpgaStateFlags::PATTERN_BANK) {
            PatternBank::B1
        } else {
            PatternBank::B0
        }
    }

    #[must_use]
    pub const fn is_pattern_mode(self) -> bool {
        self.flags().contains(FpgaStateFlags::PATTERN_MODE)
    }

    #[must_use]
    pub const fn is_stm_mode(self) -> bool {
        !self.is_pattern_mode()
    }

    #[must_use]
    pub const fn is_pattern_stopped(self) -> bool {
        self.flags().contains(FpgaStateFlags::PATTERN_STOPPED)
    }

    #[must_use]
    pub const fn is_mod_stopped(self) -> bool {
        self.flags().contains(FpgaStateFlags::MOD_STOPPED)
    }

    #[must_use]
    pub const fn is_transition_pending(self) -> bool {
        self.flags().contains(FpgaStateFlags::TRANSITION_PENDING)
    }

    #[must_use]
    pub const fn is_failsafe_active(self) -> bool {
        self.flags().contains(FpgaStateFlags::FAILSAFE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Getter = fn(FpgaState) -> bool;

    const BITS: [(u8, Getter); 8] = [
        (0, FpgaState::is_thermal_asserted),
        (1, |s| s.current_mod_bank() == ModulationBank::B1),
        (2, |s| s.current_pattern_bank() == PatternBank::B1),
        (3, FpgaState::is_pattern_mode),
        (4, FpgaState::is_pattern_stopped),
        (5, FpgaState::is_mod_stopped),
        (6, FpgaState::is_transition_pending),
        (7, FpgaState::is_failsafe_active),
    ];

    #[test]
    fn each_bit_drives_only_its_own_getter() {
        let idle = FpgaState(0);
        assert_eq!(idle.current_mod_bank(), ModulationBank::B0);
        assert_eq!(idle.current_pattern_bank(), PatternBank::B0);

        for raw in 0..=u8::MAX {
            let state = FpgaState(raw);
            assert_eq!(state.raw(), raw);
            assert_eq!(state.is_stm_mode(), !state.is_pattern_mode());
            for (bit, getter) in BITS {
                assert_eq!(
                    getter(state),
                    raw & (1 << bit) != 0,
                    "state {raw:#010b} against the getter of bit {bit}"
                );
            }
        }
    }
}
