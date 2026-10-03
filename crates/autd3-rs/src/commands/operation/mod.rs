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
mod write_foci_chunk;
mod write_modulation_chunk;
mod write_modulation_fused;
mod write_pattern_buffer;
mod write_pattern_fused;
mod write_pattern_phase;

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
pub(crate) use write_foci_chunk::WriteFociChunk;
pub(crate) use write_modulation_chunk::WriteModulationChunk;
pub(crate) use write_modulation_fused::WriteModulationFused;
pub(crate) use write_pattern_buffer::WritePatternBuffers;
pub use write_pattern_buffer::{PatternIntensity, WritePatternBuffer};
pub(crate) use write_pattern_fused::{WriteFociStmFused, WritePatternFused};
pub use write_pattern_phase::{PhaseDepth, WritePatternPhase};

use zerocopy::{Immutable, IntoBytes};

use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::mirror::FirmwareState;
use crate::params::BUFFER_SIZE_MIN;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::LoopBehavior;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoded {
    pub cmd: Cmd,
    pub len: usize,
}

impl Encoded {
    #[must_use]
    pub const fn new(cmd: Cmd, len: usize) -> Self {
        Self { cmd, len }
    }

    #[must_use]
    pub const fn no_payload(cmd: Cmd) -> Self {
        Self::new(cmd, 0)
    }

    #[must_use]
    pub const fn header<H>(cmd: Cmd) -> Self {
        Self::new(cmd, size_of::<H>())
    }

    #[must_use]
    pub const fn header_with_data<H>(cmd: Cmd, data_len: usize) -> Self {
        Self::new(cmd, size_of::<H>() + data_len)
    }
}

pub(crate) fn write_header<'a, H: IntoBytes + Immutable>(
    out: &'a mut [u8; PAYLOAD_BYTES],
    header: &H,
) -> &'a mut [u8] {
    let (dst, rest) = out.split_at_mut(core::mem::size_of::<H>());
    header.write_to(dst).expect("header fits in the payload");
    rest
}

pub(crate) fn check_index_advance(size: usize, loop_behavior: LoopBehavior) -> Result<(), Error> {
    if size < BUFFER_SIZE_MIN && !matches!(loop_behavior, LoopBehavior::Infinite) {
        return Err(PayloadError::FiniteLoopNeedsMultipleSamples { size }.into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribution {
    Broadcast,
    PerDevice,
}

pub trait Operation: crate::sealed::Sealed {
    fn distribution(&self) -> Distribution;

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error>;

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        let _ = (device, state);
        Ok(())
    }

    fn apply_clock_offset(&mut self, offset_ns: i64) {
        let _ = offset_ns;
    }
}
