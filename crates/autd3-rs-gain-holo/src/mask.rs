use autd3_rs_core::geometry::{Device, Geometry};
use autd3_rs_core::params::NUM_TRANSDUCERS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransducerMask<'a> {
    #[default]
    AllEnabled,
    Masked(&'a [[bool; NUM_TRANSDUCERS]]),
}

impl TransducerMask<'_> {
    pub(crate) fn validate(self, geometry: &Geometry) {
        if let TransducerMask::Masked(m) = self {
            assert_eq!(
                m.len(),
                geometry.len(),
                "mask must have one slot per device"
            );
        }
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
            TransducerMask::AllEnabled => geometry.iter().map(Device::len).sum(),
            TransducerMask::Masked(m) => m.iter().flatten().filter(|&&b| b).count(),
        }
    }
}
