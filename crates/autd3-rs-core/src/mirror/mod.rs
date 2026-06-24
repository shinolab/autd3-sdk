mod silencer;

pub use silencer::{
    FREQ_DIV_NO_LIMIT, SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY,
    SILENCER_DEFAULT_COMPLETION_STEPS_PHASE, SilencerAxis, SilencerGuardState, SilencerViolation,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmwareState {
    pub silencer: SilencerGuardState,
}

impl FirmwareState {
    #[must_use]
    pub fn boot_default() -> Self {
        Self {
            silencer: SilencerGuardState::boot_default(),
        }
    }
}
