const BIT_THERMAL_ASSERT: u8 = 0;
const BIT_READS_ENABLED: u8 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FpgaState(pub u8);

impl FpgaState {
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn is_thermal_asserted(self) -> bool {
        self.0 & (1 << BIT_THERMAL_ASSERT) != 0
    }

    #[must_use]
    pub const fn reads_enabled(self) -> bool {
        self.0 & (1 << BIT_READS_ENABLED) != 0
    }
}
