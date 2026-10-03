use zerocopy::{FromBytes, Immutable, KnownLayout, TryFromBytes};

use crate::Error;

mod activate_mod_bank;
mod activate_pattern_bank;
mod config_mod;
mod config_pattern;
mod firmware_info;
mod force_fan;
mod gpio_in;
mod gpio_out;
mod output_mask;
mod phase_corr;
mod pwe;
mod set_mode;
mod silencer;
mod transition_mode;
mod update_begin;
mod update_chunk;
mod write_foci;
mod write_mod;
mod write_pattern_phase;
mod write_pattern_raw;

pub use activate_mod_bank::ActivateModBankPayload;
pub use activate_pattern_bank::ActivatePatternBankPayload;
pub use config_mod::ConfigModPayload;
pub use config_pattern::{ConfigPatternPayload, EmissionType};
pub use firmware_info::FirmwareInfo;
pub use force_fan::ForceFanPayload;
pub use gpio_in::GpioInPayload;
pub use gpio_out::GpioOutPayload;
pub use output_mask::OutputMaskPayload;
pub use phase_corr::PhaseCorrPayload;
pub use pwe::PwePayload;
pub use set_mode::SetModePayload;
pub use silencer::{
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    SILENCER_DEFAULT_UPDATE_RATE, SILENCER_FLAG_BIT_STRICT_MODE, SILENCER_FLAG_STRICT_MODE,
    SilencerPayload,
};
pub use transition_mode::TransitionMode;
pub use update_begin::UpdateBeginPayload;
pub use update_chunk::UpdateChunkPayload;
pub use write_foci::WriteFociPayload;
pub use write_mod::WriteModPayload;
pub use write_pattern_phase::{PhaseDepth, WritePatternPhasePayload};
pub use write_pattern_raw::WritePatternRawPayload;

pub fn expect_empty(payload: &[u8]) -> Result<(), Error> {
    if payload.is_empty() {
        Ok(())
    } else {
        Err(Error::InvalidPayload)
    }
}

fn read_exact<T: FromBytes + KnownLayout + Immutable>(payload: &[u8]) -> Result<T, Error> {
    T::read_from_bytes(payload).map_err(|_| Error::InvalidPayload)
}

fn try_read_exact<T: TryFromBytes + KnownLayout + Immutable>(payload: &[u8]) -> Result<T, Error> {
    T::try_read_from_bytes(payload).map_err(|_| Error::InvalidPayload)
}

fn read_header<T: FromBytes + KnownLayout + Immutable>(
    payload: &[u8],
) -> Result<(T, &[u8]), Error> {
    T::read_from_prefix(payload).map_err(|_| Error::InvalidPayload)
}

fn try_read_header<T: TryFromBytes + KnownLayout + Immutable>(
    payload: &[u8],
) -> Result<(T, &[u8]), Error> {
    T::try_read_from_prefix(payload).map_err(|_| Error::InvalidPayload)
}
