use zerocopy::{Immutable, KnownLayout, TryFromBytes};

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
mod set_cpu_config;
mod silencer;
mod update_begin;
mod update_chunk;
mod write_foci;
mod write_mod;
mod write_pattern_phase;
mod write_pattern_raw;

pub use crate::fpga_params::{EmissionType, GpioOutType, SilencerFlags, TransitionMode};
pub use activate_mod_bank::ActivateModBankPayload;
pub use activate_pattern_bank::ActivatePatternBankPayload;
pub use config_mod::ConfigModPayload;
pub use config_pattern::ConfigPatternPayload;
pub use firmware_info::FirmwareInfo;
pub use force_fan::ForceFanPayload;
pub use gpio_in::GpioInPayload;
pub use gpio_out::GpioOutPayload;
pub use output_mask::OutputMaskPayload;
pub use phase_corr::PhaseCorrPayload;
pub use pwe::PwePayload;
pub use set_cpu_config::{CpuConfigOutOfRange, SetCpuConfigPayload};
pub use silencer::{
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    SILENCER_DEFAULT_UPDATE_RATE, SilencerPayload,
};
pub use update_begin::UpdateBeginPayload;
pub use update_chunk::UpdateChunkPayload;
pub use write_foci::WriteFociPayload;
pub use write_mod::WriteModPayload;
pub use write_pattern_phase::{PhaseDepth, WritePatternPhasePayload};
pub use write_pattern_raw::WritePatternRawPayload;

fn try_read_exact<T: TryFromBytes + KnownLayout + Immutable>(payload: &[u8]) -> Result<T, Error> {
    T::try_read_from_bytes(payload).map_err(|_| Error::InvalidPayload)
}

fn try_read_header<T: TryFromBytes + KnownLayout + Immutable>(
    payload: &[u8],
) -> Result<(T, &[u8]), Error> {
    T::try_read_from_prefix(payload).map_err(|_| Error::InvalidPayload)
}
