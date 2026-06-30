mod silencer;
mod transition;

pub use silencer::{
    FREQ_DIV_NO_LIMIT, SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY,
    SILENCER_DEFAULT_COMPLETION_STEPS_PHASE, SilencerAxis, SilencerGuardState, SilencerViolation,
};
pub use transition::{BankLoop, TransitionGuardState, TransitionViolation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmwareState {
    pub silencer: SilencerGuardState,
    pub transition: TransitionGuardState,
}

impl FirmwareState {
    #[must_use]
    pub fn boot_default() -> Self {
        Self {
            silencer: SilencerGuardState::boot_default(),
            transition: TransitionGuardState::boot_default(),
        }
    }
}
