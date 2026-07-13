#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Telemetry {
    FifoDrop = 0x00,
    Dedup = 0x01,
    SeqMismatch = 0x02,
    DispatchError = 0x03,
    Processed = 0x04,
    Failsafe = 0x05,
}

impl Telemetry {
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

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
    fn telemetry_ids_match_the_wire_protocol() {
        assert_eq!(Telemetry::FifoDrop.as_u8(), 0x00);
        assert_eq!(Telemetry::Dedup.as_u8(), 0x01);
        assert_eq!(Telemetry::SeqMismatch.as_u8(), 0x02);
        assert_eq!(Telemetry::DispatchError.as_u8(), 0x03);
        assert_eq!(Telemetry::Processed.as_u8(), 0x04);
        assert_eq!(Telemetry::Failsafe.as_u8(), 0x05);
    }

    #[test]
    fn fpga_functions_raw_roundtrip() {
        assert_eq!(FpgaFunctions(0xA5).raw(), 0xA5);
        assert_eq!(FpgaFunctions::UNKNOWN.raw(), 0);
    }
}
