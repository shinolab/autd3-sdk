use core::fmt;

use autd3_cpu_wire::params::FUNC_EMULATOR_BIT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl Version {
    pub const UNKNOWN: Self = Self {
        major: 0,
        minor: 0,
        patch: 0,
    };

    #[must_use]
    pub const fn is_unknown(self) -> bool {
        self.major == 0 && self.minor == 0 && self.patch == 0
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmwareVersion {
    pub cpu: Version,
    pub fpga: Version,
    pub(crate) function_bits: u8,
}

impl FirmwareVersion {
    #[must_use]
    pub const fn is_emulator(&self) -> bool {
        self.function_bits & (1 << FUNC_EMULATOR_BIT) != 0
    }
}

impl fmt::Display for FirmwareVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CPU: {}, FPGA: ", self.cpu)?;
        if self.fpga.is_unknown() {
            f.write_str("unknown")?;
        } else {
            write!(f, "{}", self.fpga)?;
        }
        if self.is_emulator() {
            f.write_str(" [Emulator]")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: Version = Version {
        major: 4,
        minor: 5,
        patch: 6,
    };

    #[test]
    fn is_emulator() {
        assert!(
            FirmwareVersion {
                cpu: V,
                fpga: V,
                function_bits: 1 << 7,
            }
            .is_emulator()
        );
        assert!(
            !FirmwareVersion {
                cpu: V,
                fpga: V,
                function_bits: 0x7F,
            }
            .is_emulator()
        );
    }

    #[test]
    fn display_appends_emulator_suffix() {
        assert_eq!(
            FirmwareVersion {
                cpu: Version {
                    major: 1,
                    minor: 2,
                    patch: 3,
                },
                fpga: V,
                function_bits: 1 << 7,
            }
            .to_string(),
            "CPU: 1.2.3, FPGA: 4.5.6 [Emulator]"
        );
    }

    #[test]
    fn display_without_emulator_bit_has_no_suffix() {
        assert_eq!(
            FirmwareVersion {
                cpu: Version {
                    major: 1,
                    minor: 2,
                    patch: 3,
                },
                fpga: V,
                function_bits: 0,
            }
            .to_string(),
            "CPU: 1.2.3, FPGA: 4.5.6"
        );
    }

    #[test]
    fn display_unknown_fpga_with_emulator_bit() {
        assert_eq!(
            FirmwareVersion {
                cpu: Version {
                    major: 1,
                    minor: 2,
                    patch: 3,
                },
                fpga: Version::UNKNOWN,
                function_bits: 1 << 7,
            }
            .to_string(),
            "CPU: 1.2.3, FPGA: unknown [Emulator]"
        );
    }
}
