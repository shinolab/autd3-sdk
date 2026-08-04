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
}
