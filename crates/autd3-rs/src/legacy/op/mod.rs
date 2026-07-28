mod basic;
mod extra;
mod foci_stm;
mod gain;
mod gain_stm;
mod modulation;
mod segment;
mod silencer;

pub use extra::SetOutputMask;
pub use foci_stm::{FociStm, FociStmOption};
pub use gain::Gain;
pub use gain_stm::{GainStm, GainStmOption};
pub use modulation::{Modulation, ModulationOption};
pub use silencer::{Silencer, SilencerConfig};

pub(crate) use basic::{Clear, FirmInfo, ForceFan, Nop, ReadsFpgaState, Sync};
pub(crate) use extra::{EmulateGpioIn, SetGpioOut, SetPhaseCorrection, SetPulseWidthTable};
pub(crate) use segment::LegacyChangePatternBank;

use autd3_rs_core::geometry::Device;

use crate::legacy::error::LegacyError;

pub(crate) trait LegacyOperation {
    fn required_size(&self, device: &Device) -> usize;
    fn pack(&mut self, device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError>;
    fn is_done(&self) -> bool;
}

#[cfg(test)]
pub(crate) fn test_frames<'a, O: LegacyOperation + Clone + 'a>(
    geometry: &autd3_rs_core::geometry::Geometry,
    op: O,
) -> Result<crate::legacy::datagram::LegacyFrames, LegacyError> {
    let mut builder =
        crate::legacy::datagram::LegacyDatagramBuilder::new(std::sync::Arc::new(geometry.clone()));
    builder.push_op(op);
    builder.build()
}
