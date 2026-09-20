use autd3_rs_core::geometry::TransducerMaskError;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum HoloError {
    #[error("at least one focus is required")]
    NoFoci,
    #[error("at least one problem is required")]
    NoProblems,
    #[error("{foci} foci cannot be split evenly across {problems} problems")]
    BatchSizeMismatch { foci: usize, problems: usize },
    #[error("the mask has {got} device slots but the geometry has {expected} devices")]
    MaskDeviceCountMismatch { got: usize, expected: usize },
    #[error(
        "the mask slot for device {device} has {got} transducers but the device has {expected}"
    )]
    MaskTransducerCountMismatch {
        device: usize,
        got: usize,
        expected: usize,
    },
    #[error("dst has {got} device slots but the geometry has {expected} devices")]
    DstDeviceCountMismatch { got: usize, expected: usize },
    #[error("dst has {phases} phase problem(s) but {intensities} intensity problem(s)")]
    DstProblemCountMismatch { phases: usize, intensities: usize },
    #[error(transparent)]
    Mask(TransducerMaskError),
}

impl From<TransducerMaskError> for HoloError {
    fn from(e: TransducerMaskError) -> Self {
        match e {
            TransducerMaskError::DeviceCountMismatch { got, expected } => {
                Self::MaskDeviceCountMismatch { got, expected }
            }
            TransducerMaskError::TransducerCountMismatch {
                device,
                got,
                expected,
            } => Self::MaskTransducerCountMismatch {
                device,
                got,
                expected,
            },
            e => Self::Mask(e),
        }
    }
}
