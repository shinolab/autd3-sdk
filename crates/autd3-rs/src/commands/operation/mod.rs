mod activate_mod_bank;
mod activate_pattern_bank;
mod config_modulation;
mod config_pattern;
mod control;
mod emulate_gpio_in;
mod force_fan;
mod set_cpu_config;
mod set_fpga_config;
mod set_gpio_out;
mod set_output_mask;
mod set_phase_correction;
mod set_pulse_width_table;
mod set_silencer;
mod write_foci_buffer;
mod write_modulation_buffer;
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
pub use set_cpu_config::{
    CpuConfig, FpgaBusWait, MaxFreqPpb, PathDelayFilterShift, PtpConfig, SetCpuConfig,
};
pub use set_fpga_config::{FpgaConfig, SetFpgaConfig};
pub use set_gpio_out::{GpioOut, SetGpioOut};
pub use set_output_mask::SetOutputMask;
pub use set_phase_correction::SetPhaseCorrection;
pub use set_pulse_width_table::{PWE_TABLE_SIZE, SetPulseWidthTable};
pub use set_silencer::{FixedCompletionTime, FixedUpdateRate, SetSilencer, SilencerConfig};
pub use write_foci_buffer::WriteFociBuffer;
pub(crate) use write_foci_buffer::num_foci;
pub use write_modulation_buffer::WriteModulationBuffer;
pub(crate) use write_pattern_buffer::encode_raw_pattern_entry;
pub use write_pattern_buffer::{StmIntensity, WritePatternBuffer};
pub use write_pattern_phase::{PhaseDepth, WritePatternPhase};

use zerocopy::{Immutable, IntoBytes};

use crate::datagram::Datagram;
use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::protocol::Cmd;

pub(crate) fn set_cmd(out: &mut Datagram, cmd: Cmd, payload_len: usize) {
    out.cmd = cmd;
    out.payload_len = payload_len;
}

pub(crate) fn write_header<'a, H: IntoBytes + Immutable>(
    out: &'a mut Datagram,
    header: &H,
) -> &'a mut [u8] {
    let (dst, rest) = out.payload.split_at_mut(core::mem::size_of::<H>());
    header.write_to(dst).expect("header fits in the payload");
    rest
}

pub(crate) fn encode_header_only<H: IntoBytes + Immutable>(
    out: &mut Datagram,
    cmd: Cmd,
    header: &H,
) {
    write_header(out, header);
    set_cmd(out, cmd, size_of::<H>());
}

pub(crate) fn checked_device_row<'a, T>(
    data: &'a [Vec<T>],
    device: &Device,
) -> Result<&'a [T], Error> {
    let row = data
        .get(device.idx())
        .ok_or(PayloadError::DeviceDataOutOfRange {
            device: device.idx(),
            len: data.len(),
        })?;
    if row.len() != device.num_transducers() {
        return Err(PayloadError::TransducerCountMismatch {
            device: device.idx(),
            got: row.len(),
            expected: device.num_transducers(),
        }
        .into());
    }
    Ok(row)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribution {
    Broadcast,
    PerDevice,
}

impl Distribution {
    pub(crate) const fn num_datagrams(self, num_devices: usize) -> usize {
        match self {
            Self::Broadcast => 1,
            Self::PerDevice => num_devices,
        }
    }
}

pub trait Operation: crate::sealed::Sealed {
    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(&self, device: &Device, out: &mut Datagram) -> Result<(), Error>;
}

impl<O: Operation + ?Sized> crate::sealed::Sealed for Box<O> {}

impl<O: Operation + ?Sized> Operation for Box<O> {
    fn distribution(&self) -> Distribution {
        (**self).distribution()
    }

    fn encode(&self, device: &Device, out: &mut Datagram) -> Result<(), Error> {
        (**self).encode(device, out)
    }
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
            &WriteFociBuffer {
                bank: PatternBank::B0,
                start_index: 0,
                points: &[],
            },
            &WritePatternBuffer::new(PatternBank::B0, 0, &[], Intensity::MAX),
            &crate::commands::Pattern::new(&[], Intensity::MAX),
            &WritePatternPhase {
                bank: PatternBank::B0,
                start_index: 0,
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
