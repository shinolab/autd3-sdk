use core::fmt;

use autd3_cpu_wire::fpga_params::{FunctionBits, VERSION_NUM_MAJOR, VERSION_NUM_MINOR};
use autd3_cpu_wire::payload::FirmwareInfo;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl Version {
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
    pub const SUPPORTED_SERIES: (u8, u8) = (VERSION_NUM_MAJOR, VERSION_NUM_MINOR);

    pub(crate) fn from_info(info: FirmwareInfo) -> Self {
        let [major, minor, patch] = info.cpu_version;
        let cpu = Version {
            major,
            minor,
            patch,
        };
        let [major, minor, patch] = info.fpga_version;
        let fpga = Version {
            major,
            minor,
            patch,
        };
        Self {
            cpu,
            fpga,
            function_bits: info.fpga_functions,
        }
    }

    #[must_use]
    pub const fn is_emulator(&self) -> bool {
        FunctionBits::from_bits_retain(self.function_bits).contains(FunctionBits::EMULATOR)
    }

    #[must_use]
    pub const fn is_supported(&self) -> bool {
        !self.fpga.is_unknown() && is_supported_series(self.cpu) && is_supported_series(self.fpga)
    }
}

const fn is_supported_series(version: Version) -> bool {
    version.major == FirmwareVersion::SUPPORTED_SERIES.0
        && version.minor == FirmwareVersion::SUPPORTED_SERIES.1
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

    const UNKNOWN: Version = Version {
        major: 0,
        minor: 0,
        patch: 0,
    };

    const V: Version = Version {
        major: 4,
        minor: 5,
        patch: 6,
    };

    const CPU: Version = Version {
        major: 1,
        minor: 2,
        patch: 3,
    };

    fn in_series(major: u8, minor: u8) -> Version {
        Version {
            major,
            minor,
            patch: 9,
        }
    }

    fn firmware(cpu: Version, fpga: Version, function_bits: u8) -> FirmwareVersion {
        FirmwareVersion {
            cpu,
            fpga,
            function_bits,
        }
    }

    #[test]
    fn the_bundled_series_is_supported() {
        let (major, minor) = FirmwareVersion::SUPPORTED_SERIES;
        let bundled = in_series(major, minor);
        assert!(firmware(bundled, bundled, 0).is_supported());
    }

    #[test]
    fn a_foreign_series_is_unsupported() {
        let (major, minor) = FirmwareVersion::SUPPORTED_SERIES;
        let bundled = in_series(major, minor);
        for foreign in [
            in_series(major, minor.wrapping_add(1)),
            in_series(major.wrapping_add(1), minor),
        ] {
            assert!(!firmware(foreign, bundled, 0).is_supported());
            assert!(!firmware(bundled, foreign, 0).is_supported());
        }
    }

    #[test]
    fn an_unknown_fpga_version_is_never_supported() {
        let (major, minor) = FirmwareVersion::SUPPORTED_SERIES;
        let unknown = firmware(in_series(major, minor), UNKNOWN, 0);
        assert!(unknown.fpga.is_unknown());
        assert!(!unknown.is_supported());
    }

    #[test]
    fn is_emulator() {
        assert!(firmware(V, V, 1 << 7).is_emulator());
        assert!(!firmware(V, V, 0x7F).is_emulator());
    }

    #[test]
    fn display_appends_emulator_suffix() {
        assert_eq!(
            firmware(CPU, V, 1 << 7).to_string(),
            "CPU: 1.2.3, FPGA: 4.5.6 [Emulator]"
        );
    }

    #[test]
    fn display_without_emulator_bit_has_no_suffix() {
        assert_eq!(firmware(CPU, V, 0).to_string(), "CPU: 1.2.3, FPGA: 4.5.6");
    }

    #[test]
    fn display_unknown_fpga_with_emulator_bit() {
        assert_eq!(
            firmware(CPU, UNKNOWN, 1 << 7).to_string(),
            "CPU: 1.2.3, FPGA: unknown [Emulator]"
        );
    }
}
