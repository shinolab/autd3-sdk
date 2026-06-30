mod change_mod_bank;
mod change_pattern_bank;
mod clear;
mod config_modulation;
mod config_pattern;
mod emulate_gpio_in;
mod force_fan;
mod nop;
mod set_gpio_out;
mod set_output_mask;
mod set_phase_correction;
mod set_pulse_width_table;
mod set_silencer;
mod synchronize;
mod write_foci_buffer;
mod write_modulation_buffer;
mod write_pattern_buffer;
mod write_pattern_compressed;
mod xor_hash;

pub use change_mod_bank::ChangeModulationBank;
pub use change_pattern_bank::ChangePatternBank;
pub use clear::Clear;
pub use config_modulation::ConfigModulation;
pub use config_pattern::{ConfigFociStm, ConfigPattern};
pub use emulate_gpio_in::EmulateGpioIn;
pub use force_fan::ForceFan;
pub use nop::Nop;
pub use set_gpio_out::{GpioOut, SetGpioOut};
pub use set_output_mask::SetOutputMask;
pub use set_phase_correction::SetPhaseCorrection;
pub use set_pulse_width_table::{PWE_TABLE_SIZE, SetPulseWidthTable};
pub use set_silencer::{FixedCompletionTime, FixedUpdateRate, SetSilencer, SilencerConfig};
pub use synchronize::Synchronize;
pub use write_foci_buffer::WriteFociBuffer;
pub use write_modulation_buffer::WriteModulationBuffer;
pub use write_pattern_buffer::WritePatternBuffer;
pub use write_pattern_compressed::{
    PATTERN_MAX_PER_FRAME, PatternCompression, WritePatternCompressed,
};
pub use xor_hash::{XOR_HASH_HEADER_BYTES, XOR_HASH_MAX_DATA_LEN, XorHashCmd};

use crate::error::Error;
use crate::mirror::FirmwareState;
use crate::params::FOCUS_WORDS;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

pub(crate) fn silencer_constraint(
    device: usize,
    violation: autd3_rs_core::SilencerViolation,
) -> Error {
    Error::SilencerConstraint {
        device,
        axis: violation.axis,
        completion_steps: violation.completion_steps,
        sampling_div: violation.sampling_div,
    }
}

pub(crate) fn transition_constraint(
    device: usize,
    violation: autd3_rs_core::TransitionViolation,
) -> Error {
    Error::TransitionConstraint {
        device,
        transition_mode: violation.transition_mode,
        bank_loop: violation.bank_loop,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribution {
    Broadcast,
    PerDevice,
}

pub trait Operation {
    fn frames(&self) -> usize;

    fn distribution(&self) -> Distribution;

    fn encode(
        &self,
        device: usize,
        frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error>;

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        let _ = (device, state);
        Ok(())
    }
}

impl<T: Operation + ?Sized> Operation for &T {
    fn frames(&self) -> usize {
        (**self).frames()
    }

    fn distribution(&self) -> Distribution {
        (**self).distribution()
    }

    fn encode(
        &self,
        device: usize,
        frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        (**self).encode(device, frame, out)
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        (**self).reflect(device, state)
    }
}

pub const WRITE_HEADER_BYTES: usize = 8;

pub const WRITE_MAX_DATA_LEN: usize = PAYLOAD_BYTES - WRITE_HEADER_BYTES;

pub const MAX_FOCI_PER_FRAME: usize = WRITE_MAX_DATA_LEN / (FOCUS_WORDS * 2);
