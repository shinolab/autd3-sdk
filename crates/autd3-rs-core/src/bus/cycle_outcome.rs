#[derive(Debug, Clone, Copy)]
pub struct CycleOutcome {
    rx_valid: bool,
}

impl CycleOutcome {
    #[must_use]
    pub const fn valid() -> Self {
        Self { rx_valid: true }
    }

    #[must_use]
    pub const fn stale() -> Self {
        Self { rx_valid: false }
    }

    #[must_use]
    pub const fn rx_valid(self) -> bool {
        self.rx_valid
    }
}
