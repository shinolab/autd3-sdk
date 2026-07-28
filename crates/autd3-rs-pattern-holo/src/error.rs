use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum HoloError {
    #[error("at least one focus (control point) is required")]
    NoFoci,
    #[error("at least one problem is required")]
    NoProblems,
    #[error("every problem in a batch must have the same focus count: found {0} and {1}")]
    UnevenBatch(usize, usize),
    #[error("the batch has {problems} problems but {slots} output slots")]
    BatchSizeMismatch { problems: usize, slots: usize },
}
