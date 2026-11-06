use autd3_cpu_wire::update::RunningImage;
use autd3_rs::{Error, Version};

use crate::verify::describe_running_image;

#[derive(Debug, thiserror::Error)]
#[error(
    "device {device} runs CPU firmware {found} after the reboot, but {expected}; the trial image is not the one running, so it is left unconfirmed and the previous image stays"
)]
pub(crate) struct TrialNotRunning {
    pub(crate) device: usize,
    pub(crate) found: Version,
    pub(crate) expected: String,
}

#[derive(Debug)]
pub(crate) struct OffTrial {
    pub(crate) device: usize,
    pub(crate) version: Version,
    pub(crate) image: Option<RunningImage>,
}

impl std::fmt::Display for OffTrial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "device {} ({}, CPU firmware {})",
            self.device,
            describe_running_image(self.image),
            self.version
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "not every device runs the trial image after the reboot: {} fell back to the previous image. \
     No device was confirmed, so the devices that do run the trial image fall back as well at \
     their next reset; run the update again, or reset every device (`autd3-firmware-ota reboot` or a power \
     cycle) to bring them all back to the previous image",
    devices.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
)]
pub(crate) struct NotOnTrial {
    pub(crate) devices: Vec<OffTrial>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OtaError {
    #[error(transparent)]
    Autd3(#[from] Error),
    #[error("not every device replied within {0:?}")]
    Timeout(std::time::Duration),
    #[error(
        "device {device} runs an FPGA image without configuration-flash access; flash `autd3-fpga.mcs` once via JTAG"
    )]
    FpgaUpdateUnsupported { device: usize },
    #[error(
        "device {device} did not reconfigure after activation; it keeps its output off and boots the written image at the next power cycle"
    )]
    FpgaReconfigFailed { device: usize },
    #[error(
        "device {device} runs CPU firmware {found}, but the update needs {required} or newer; flash it once via J-Link"
    )]
    UnsupportedFirmware {
        device: usize,
        found: Version,
        required: Version,
    },
}
