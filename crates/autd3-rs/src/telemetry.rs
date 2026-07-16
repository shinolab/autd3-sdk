pub use autd3_cpu_wire::Telemetry;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FpgaFunctions(pub u8);

impl FpgaFunctions {
    pub const UNKNOWN: Self = Self(0);

    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fpga_functions_raw_roundtrip() {
        assert_eq!(FpgaFunctions(0xA5).raw(), 0xA5);
        assert_eq!(FpgaFunctions::UNKNOWN.raw(), 0);
    }
}
