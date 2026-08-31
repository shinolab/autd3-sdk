use autd3_rs_core::geometry::{Device, Geometry};

use crate::error::HoloError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TransducerMask<'a> {
    #[default]
    AllEnabled,
    Masked(&'a [Vec<bool>]),
}

impl TransducerMask<'_> {
    pub(crate) fn validate(self, geometry: &Geometry) -> Result<(), HoloError> {
        let TransducerMask::Masked(m) = self else {
            return Ok(());
        };
        if m.len() != geometry.num_devices() {
            return Err(HoloError::MaskDeviceCountMismatch {
                got: m.len(),
                expected: geometry.num_devices(),
            });
        }
        for (device, (slot, dev)) in m.iter().zip(geometry.iter()).enumerate() {
            if slot.len() != dev.num_transducers() {
                return Err(HoloError::MaskTransducerCountMismatch {
                    device,
                    got: slot.len(),
                    expected: dev.num_transducers(),
                });
            }
        }
        Ok(())
    }

    #[must_use]
    pub(crate) fn is_enabled(self, device: usize, transducer: usize) -> bool {
        match self {
            TransducerMask::AllEnabled => true,
            TransducerMask::Masked(m) => m[device][transducer],
        }
    }

    #[must_use]
    pub(crate) fn num_enabled(self, geometry: &Geometry) -> usize {
        match self {
            TransducerMask::AllEnabled => geometry.iter().map(Device::num_transducers).sum(),
            TransducerMask::Masked(m) => m.iter().flatten().filter(|&&b| b).count(),
        }
    }
}

pub(crate) fn validate_dst_len(dst: usize, geometry: &Geometry) -> Result<(), HoloError> {
    if dst != geometry.num_devices() {
        return Err(HoloError::DstDeviceCountMismatch {
            got: dst,
            expected: geometry.num_devices(),
        });
    }
    Ok(())
}
