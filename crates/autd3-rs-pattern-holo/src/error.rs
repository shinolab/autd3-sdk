use autd3_rs_core::geometry::TransducerMaskError;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum HoloError {
    #[error("at least one focus is required")]
    NoFoci,
    #[error("dst has {got} device slots but the geometry has {expected} devices")]
    DstDeviceCountMismatch { got: usize, expected: usize },
    #[error("intensities has {got} device slots but the geometry has {expected} devices")]
    IntensityDeviceCountMismatch { got: usize, expected: usize },
    #[error(
        "intensities has {got} transducers for device {device} but the geometry has {expected}"
    )]
    IntensityTransducerCountMismatch {
        device: usize,
        got: usize,
        expected: usize,
    },
    #[error("order contains index {index} but the geometry has {num_transducers} transducers")]
    OrderIndexOutOfRange {
        index: usize,
        num_transducers: usize,
    },
    #[error(transparent)]
    Mask(#[from] TransducerMaskError),
}
