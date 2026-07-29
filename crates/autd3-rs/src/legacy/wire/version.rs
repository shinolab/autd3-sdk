use super::params::CPU_VERSION_V12_1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}

impl core::fmt::Display for Version {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (major, minor) = (self.major, self.minor);
        match major {
            0 => write!(f, "older than legacy-v0.4"),
            0x01..=0x06 => write!(f, "legacy-v0.{}", major + 3),
            0x0A..=0x15 => write!(f, "legacy-v1.{}", major - 0x0A),
            0x80..=0x89 => write!(f, "legacy-v2.{}.{}", major - 0x80, minor),
            0x8A => write!(f, "legacy-v3.0.{minor}"),
            0x8B..=0x8C => write!(f, "legacy-v4.{}.{}", major - 0x8B, minor),
            0x8D..=0x8E => write!(f, "legacy-v5.{}.{}", major - 0x8D, minor),
            0x8F..=0x90 => write!(f, "legacy-v6.{}.{}", major - 0x8F, minor),
            0x91 => write!(f, "legacy-v7.0.{minor}"),
            0x92 => write!(f, "legacy-v8.0.{minor}"),
            0xA0..=0xA1 => write!(f, "legacy-v9.{}.{}", major - 0xA0, minor),
            0xA2 => write!(f, "legacy-v10.0.{minor}"),
            0xA3 => write!(f, "legacy-v11.0.{minor}"),
            0xA4..=0xA5 => write!(f, "legacy-v12.{}.{}", major - 0xA4, minor),
            _ => write!(f, "unknown legacy version ({major})"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmwareVersion {
    pub idx: usize,
    pub cpu: Version,
    pub fpga: Version,
    pub function_bits: u8,
}

impl FirmwareVersion {
    pub const ENABLED_EMULATOR_BIT: u8 = 1 << 7;

    pub const SUPPORTED_CPU_MAJOR: u8 = CPU_VERSION_V12_1;

    #[must_use]
    pub const fn is_emulator(&self) -> bool {
        (self.function_bits & Self::ENABLED_EMULATOR_BIT) == Self::ENABLED_EMULATOR_BIT
    }

    #[must_use]
    pub const fn is_supported(&self) -> bool {
        self.cpu.major == Self::SUPPORTED_CPU_MAJOR
    }
}

impl core::fmt::Display for FirmwareVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: CPU = {}, FPGA = {}", self.idx, self.cpu, self.fpga)?;
        if self.is_emulator() {
            write!(f, " [Emulator]")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u8) -> String {
        Version { major, minor: 0 }.to_string()
    }

    #[test]
    fn version_map_matches_legacy_sdk() {
        assert_eq!(v(0), "older than legacy-v0.4");
        assert_eq!(v(1), "legacy-v0.4");
        assert_eq!(v(6), "legacy-v0.9");
        assert_eq!(v(7), "unknown legacy version (7)");
        assert_eq!(v(10), "legacy-v1.0");
        assert_eq!(v(21), "legacy-v1.11");
        assert_eq!(v(128), "legacy-v2.0.0");
        assert_eq!(v(137), "legacy-v2.9.0");
        assert_eq!(v(138), "legacy-v3.0.0");
        assert_eq!(v(139), "legacy-v4.0.0");
        assert_eq!(v(140), "legacy-v4.1.0");
        assert_eq!(v(141), "legacy-v5.0.0");
        assert_eq!(v(142), "legacy-v5.1.0");
        assert_eq!(v(143), "legacy-v6.0.0");
        assert_eq!(v(144), "legacy-v6.1.0");
        assert_eq!(v(145), "legacy-v7.0.0");
        assert_eq!(v(146), "legacy-v8.0.0");
        assert_eq!(v(147), "unknown legacy version (147)");
        assert_eq!(v(160), "legacy-v9.0.0");
        assert_eq!(v(161), "legacy-v9.1.0");
        assert_eq!(v(162), "legacy-v10.0.0");
        assert_eq!(v(163), "legacy-v11.0.0");
        assert_eq!(v(164), "legacy-v12.0.0");
        assert_eq!(v(165), "legacy-v12.1.0");
    }

    #[test]
    fn only_0xa5_is_supported() {
        let mk = |major| FirmwareVersion {
            idx: 0,
            cpu: Version { major, minor: 0 },
            fpga: Version { major, minor: 0 },
            function_bits: 0,
        };
        assert!(mk(0xA5).is_supported());
        assert!(!mk(0xA4).is_supported());
        assert!(!mk(0xA3).is_supported());
    }

    #[test]
    fn display_reports_both_cores_and_emulator_bit() {
        let mut fw = FirmwareVersion {
            idx: 2,
            cpu: Version {
                major: 0xA5,
                minor: 0,
            },
            fpga: Version {
                major: 0xA5,
                minor: 1,
            },
            function_bits: 0,
        };
        assert_eq!(
            fw.to_string(),
            "2: CPU = legacy-v12.1.0, FPGA = legacy-v12.1.1"
        );
        fw.function_bits = FirmwareVersion::ENABLED_EMULATOR_BIT;
        assert!(fw.is_emulator());
        assert_eq!(
            fw.to_string(),
            "2: CPU = legacy-v12.1.0, FPGA = legacy-v12.1.1 [Emulator]"
        );
    }
}
