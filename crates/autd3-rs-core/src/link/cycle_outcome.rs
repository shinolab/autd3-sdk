#[derive(Debug, Clone, Copy)]
pub struct CycleOutcome {
    rx_valid: bool,
}

impl CycleOutcome {
    #[must_use]
    pub const fn new(rx_valid: bool) -> Self {
        Self { rx_valid }
    }

    #[must_use]
    pub const fn rx_valid(self) -> bool {
        self.rx_valid
    }
}
