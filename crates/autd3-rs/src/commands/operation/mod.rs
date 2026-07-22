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
mod write_pattern_compressed;
mod write_pattern_fused;

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
pub use write_modulation_fused::WriteModulationFused;
pub use write_pattern_buffer::WritePatternBuffer;
pub use write_pattern_compressed::{
    PATTERN_MAX_PER_FRAME, PatternCompression, WritePatternCompressed,
};
pub use write_pattern_fused::{WriteFociStmFused, WritePatternFused};

pub use autd3_cpu_wire::layout::{MAX_FOCI_PER_FRAME, WRITE_MAX_DATA_LEN};

#[cfg(test)]
pub(crate) use write_modulation_fused::MOD_FUSED_MAX_DATA_LEN;
#[cfg(test)]
pub(crate) use write_pattern_fused::{
    PATTERN_FUSED_HEADER_BYTES, PATTERN_FUSED_MAX_FOCI_PER_FRAME,
};

use crate::error::Error;
use crate::geometry::Device;
use crate::mirror::FirmwareState;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribution {
    Broadcast,
    PerDevice,
}

pub trait Operation {
    fn distribution(&self) -> Distribution;

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error>;

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        let _ = (device, state);
        Ok(())
    }
}

impl<T: Operation + ?Sized> Operation for &T {
    fn distribution(&self) -> Distribution {
        (**self).distribution()
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        (**self).encode(device, out)
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        (**self).reflect(device, state)
    }
}
