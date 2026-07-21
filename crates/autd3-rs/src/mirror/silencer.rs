use crate::error::Error;
use crate::params::NUM_BANKS;

pub const SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY: u16 = 10;
pub const SILENCER_DEFAULT_COMPLETION_STEPS_PHASE: u16 = 40;
pub const FREQ_DIV_NO_LIMIT: u16 = 0xFFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SilencerAxis {
    Intensity,
    Phase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SilencerGuardState {
    pub strict_mode: bool,
    pub completion_intensity: u16,
    pub completion_phase: u16,
    pub mod_freq_div: [u16; NUM_BANKS],
    pub pattern_freq_div: [u16; NUM_BANKS],
    pub mod_bank: u8,
    pub pattern_bank: u8,
}

impl SilencerGuardState {
    #[must_use]
    pub fn boot_default() -> Self {
        Self {
            strict_mode: false,
            completion_intensity: SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY,
            completion_phase: SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
            mod_freq_div: [FREQ_DIV_NO_LIMIT; NUM_BANKS],
            pattern_freq_div: [FREQ_DIV_NO_LIMIT; NUM_BANKS],
            mod_bank: 0,
            pattern_bank: 0,
        }
    }

    pub fn check_mod_div(&self, device: usize, div: u16) -> Result<(), Error> {
        if self.strict_mode && div < self.completion_intensity {
            return Err(violation(
                device,
                SilencerAxis::Intensity,
                self.completion_intensity,
                div,
            ));
        }
        Ok(())
    }

    pub fn check_pattern_div(&self, device: usize, div: u16) -> Result<(), Error> {
        if !self.strict_mode {
            return Ok(());
        }
        if div < self.completion_intensity {
            return Err(violation(
                device,
                SilencerAxis::Intensity,
                self.completion_intensity,
                div,
            ));
        }
        if div < self.completion_phase {
            return Err(violation(
                device,
                SilencerAxis::Phase,
                self.completion_phase,
                div,
            ));
        }
        Ok(())
    }

    pub fn check_mod_bank(&self, device: usize, bank: u8) -> Result<(), Error> {
        self.check_mod_div(device, self.mod_freq_div[usize::from(bank)])
    }

    pub fn check_pattern_bank(&self, device: usize, bank: u8) -> Result<(), Error> {
        self.check_pattern_div(device, self.pattern_freq_div[usize::from(bank)])
    }

    pub fn check_set_strict(
        &self,
        device: usize,
        completion_intensity: u16,
        completion_phase: u16,
    ) -> Result<(), Error> {
        let mod_div = self.mod_freq_div[usize::from(self.mod_bank)];
        let pattern_div = self.pattern_freq_div[usize::from(self.pattern_bank)];
        if mod_div < completion_intensity {
            return Err(violation(
                device,
                SilencerAxis::Intensity,
                completion_intensity,
                mod_div,
            ));
        }
        if pattern_div < completion_intensity {
            return Err(violation(
                device,
                SilencerAxis::Intensity,
                completion_intensity,
                pattern_div,
            ));
        }
        if pattern_div < completion_phase {
            return Err(violation(
                device,
                SilencerAxis::Phase,
                completion_phase,
                pattern_div,
            ));
        }
        Ok(())
    }

    pub fn note_mod_div(&mut self, bank: u8, div: u16) {
        self.mod_freq_div[usize::from(bank)] = div;
    }

    pub fn note_pattern_div(&mut self, bank: u8, div: u16) {
        self.pattern_freq_div[usize::from(bank)] = div;
    }

    pub fn note_mod_bank(&mut self, bank: u8) {
        self.mod_bank = bank;
    }

    pub fn note_pattern_bank(&mut self, bank: u8) {
        self.pattern_bank = bank;
    }

    pub fn apply_completion(
        &mut self,
        completion_intensity: u16,
        completion_phase: u16,
        strict: bool,
    ) {
        self.strict_mode = strict;
        self.completion_intensity = completion_intensity;
        self.completion_phase = completion_phase;
    }

    pub fn release(&mut self) {
        self.strict_mode = false;
    }
}

fn violation(device: usize, axis: SilencerAxis, completion_steps: u16, sampling_div: u16) -> Error {
    Error::SilencerConstraint {
        device,
        axis,
        completion_steps,
        sampling_div,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_default_matches_firmware_init() {
        let g = SilencerGuardState::boot_default();
        assert!(!g.strict_mode);
        assert_eq!(g.completion_intensity, 10);
        assert_eq!(g.completion_phase, 40);
        assert_eq!(g.mod_freq_div, [0xFFFF; NUM_BANKS]);
        assert_eq!(g.pattern_freq_div, [0xFFFF; NUM_BANKS]);
        assert_eq!(g.mod_bank, 0);
        assert_eq!(g.pattern_bank, 0);
    }

    #[test]
    fn non_strict_never_violates() {
        let mut g = SilencerGuardState::boot_default();
        g.apply_completion(256, 256, false);
        assert!(g.check_mod_div(0, 1).is_ok());
        assert!(g.check_pattern_div(0, 1).is_ok());
    }

    #[test]
    fn strict_mod_div_rejects_below_intensity() {
        let mut g = SilencerGuardState::boot_default();
        g.apply_completion(10, 40, true);
        assert!(matches!(
            g.check_mod_div(3, 9),
            Err(Error::SilencerConstraint {
                device: 3,
                axis: SilencerAxis::Intensity,
                completion_steps: 10,
                sampling_div: 9,
            })
        ));
        assert!(g.check_mod_div(0, 10).is_ok(), "equal is allowed");
    }

    #[test]
    fn strict_pattern_div_checks_intensity_then_phase() {
        let mut g = SilencerGuardState::boot_default();
        g.apply_completion(10, 40, true);
        assert!(matches!(
            g.check_pattern_div(0, 9),
            Err(Error::SilencerConstraint {
                axis: SilencerAxis::Intensity,
                ..
            })
        ));
        assert!(matches!(
            g.check_pattern_div(0, 20),
            Err(Error::SilencerConstraint {
                axis: SilencerAxis::Phase,
                ..
            })
        ));
        assert!(g.check_pattern_div(0, 40).is_ok());
    }

    #[test]
    fn set_strict_checks_active_banks() {
        let mut g = SilencerGuardState::boot_default();
        g.note_mod_div(0, 5);
        assert!(matches!(
            g.check_set_strict(1, 8, 40),
            Err(Error::SilencerConstraint {
                device: 1,
                axis: SilencerAxis::Intensity,
                completion_steps: 8,
                sampling_div: 5,
            })
        ));
    }

    #[test]
    fn change_bank_uses_target_bank_divider() {
        let mut g = SilencerGuardState::boot_default();
        g.note_mod_div(1, 5);
        g.apply_completion(10, 40, true);
        assert!(g.check_mod_bank(0, 0).is_ok(), "bank 0 still no-limit");
        assert!(g.check_mod_bank(0, 1).is_err(), "bank 1 sampling too fast");
    }

    #[test]
    fn release_clears_guard() {
        let mut g = SilencerGuardState::boot_default();
        g.apply_completion(10, 40, true);
        g.release();
        assert!(g.check_mod_div(0, 1).is_ok());
    }
}
