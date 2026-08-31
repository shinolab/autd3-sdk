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
}
