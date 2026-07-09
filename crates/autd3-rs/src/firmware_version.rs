use core::fmt;

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
}

impl fmt::Display for FirmwareVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CPU: {}, FPGA: ", self.cpu)?;
        if self.fpga.is_unknown() {
            f.write_str("unknown")
        } else {
            write!(f, "{}", self.fpga)
        }
    }
}
