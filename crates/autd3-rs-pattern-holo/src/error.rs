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
    #[error("dst has {got} device slots but the geometry has {expected} devices")]
    DstDeviceCountMismatch { got: usize, expected: usize },
    #[error("dst has {phases} phase problem(s) but {intensities} intensity problem(s)")]
    DstProblemCountMismatch { phases: usize, intensities: usize },
    #[error(transparent)]
    Mask(#[from] TransducerMaskError),
}
