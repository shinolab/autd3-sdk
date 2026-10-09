mod activate_mod_bank;
mod activate_pattern_bank;
mod config_modulation;
mod config_pattern;
mod control;
mod emulate_gpio_in;
mod force_fan;
mod set_cpu_config;
mod set_gpio_out;
mod set_output_mask;
mod set_phase_correction;
mod set_pulse_width_table;
mod set_silencer;
mod write_foci_chunk;
mod write_modulation_chunk;
mod write_pattern_buffer;
mod write_pattern_phase;

pub use crate::value::PatternIntensity;
pub use activate_mod_bank::ActivateModulationBank;
pub use activate_pattern_bank::ActivatePatternBank;
pub use config_modulation::ConfigModulation;
pub use config_pattern::{ConfigFociStm, ConfigPattern};
pub use control::{Clear, Nop, ReleaseFailsafe, Synchronize};
pub use emulate_gpio_in::EmulateGpioIn;
pub use force_fan::ForceFan;
pub use set_cpu_config::{CpuConfig, FpgaBusWait, PtpConfig, SetCpuConfig};
pub use set_gpio_out::{GpioOut, SetGpioOut};
pub use set_output_mask::SetOutputMask;
pub use set_phase_correction::SetPhaseCorrection;
pub use set_pulse_width_table::{PWE_TABLE_SIZE, SetPulseWidthTable};
pub use set_silencer::{FixedCompletionTime, FixedUpdateRate, SetSilencer, SilencerConfig};
pub(crate) use write_foci_chunk::WriteFociChunk;
pub(crate) use write_modulation_chunk::WriteModulationChunk;
pub(crate) use write_pattern_buffer::WritePatternBuffers;
pub use write_pattern_buffer::{StmIntensity, WritePatternBuffer};
pub use write_pattern_phase::{PhaseDepth, WritePatternPhase};

use zerocopy::{Immutable, IntoBytes};

use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

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

pub(crate) fn encode_fixed<H: IntoBytes + Immutable>(
    out: &mut [u8; PAYLOAD_BYTES],
    cmd: Cmd,
    header: &H,
) -> Encoded {
    write_header(out, header);
    Encoded::new(cmd, size_of::<H>())
}

pub(crate) fn device_slot<'a, T>(data: &'a [Vec<T>], device: &Device) -> Result<&'a [T], Error> {
    let slot = data
        .get(device.idx())
        .ok_or(PayloadError::DeviceDataOutOfRange {
            device: device.idx(),
            len: data.len(),
        })?;
    if slot.len() != device.num_transducers() {
        return Err(PayloadError::TransducerCountMismatch {
            device: device.idx(),
            got: slot.len(),
            expected: device.num_transducers(),
        }
        .into());
    }
    Ok(slot)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribution {
    Broadcast,
    PerDevice,
}

pub trait Operation: crate::sealed::Sealed {
    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Intensity, PatternBank};

    #[test]
    fn ops_whose_payload_depends_on_the_device_are_per_device() {
        let ops: [&dyn Operation; 6] = [
            &SetOutputMask { masks: &[] },
            &SetPhaseCorrection { phases: &[] },
            &WriteFociChunk::<1> {
                bank: PatternBank::B0,
                index_offset: 0,
                points: &[],
                focus_start: 0,
                focus_len: 0,
            },
            &WritePatternBuffer::new(PatternBank::B0, 0, &[], Intensity::MAX),
            &WritePatternBuffers {
                bank: PatternBank::B0,
                index: 0,
                phases: &[],
                intensities: StmIntensity::default(),
            },
            &WritePatternPhase {
                bank: PatternBank::B0,
                index: 0,
                depth: PhaseDepth::Bits8,
                intensity: Intensity::MAX,
                patterns: &[],
            },
        ];
        for op in ops {
            assert_eq!(op.distribution(), Distribution::PerDevice);
        }
    }
}
