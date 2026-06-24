use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum HoloError {
    #[error("at least one focus (control point) is required")]
    NoFoci,
}
