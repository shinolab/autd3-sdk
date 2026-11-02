use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, ValueEnum};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum Target {
    #[default]
    Both,
    Cpu,
    Fpga,
}

impl Target {
    pub(crate) fn cpu(self) -> bool {
        matches!(self, Self::Both | Self::Cpu)
    }

    pub(crate) fn fpga(self) -> bool {
        matches!(self, Self::Both | Self::Fpga)
    }
}

#[derive(Parser)]
#[command(
    name = "autd3-firmware-ota",
    about,
    long_about = "Update the AUTD3 CPU / FPGA firmware.\n\n\
        Pass a flash image, or --version to download a release bundle (0.10.0 or newer). \
        A local image is not version-checked. The CPU image is written to the inactive slot, \
        the devices reboot into it, and the tool reconnects to confirm it (the reboot leaves \
        the devices unassigned, so the chain is enumerated again); an unconfirmed image rolls \
        back at the next reset. The FPGA image is written to the update slot and the FPGA \
        reconfigures in place (golden image fallback)."
)]
pub(crate) struct Cli {
    #[arg(
        required_unless_present_any = ["version", "reboot_only"],
        help = "Flash image: `autd3-cpu.bin` (or `*-fpga-update.img` with --fpga); its version is not checked"
    )]
    pub(crate) image: Option<PathBuf>,
    #[arg(
        long,
        conflicts_with = "image",
        help = "Release to download and write (e.g. 0.10.0; 0.10.0 or newer) instead of a local image"
    )]
    pub(crate) version: Option<String>,
    #[arg(
        long,
        value_enum,
        default_value_t = Target::Both,
        requires = "version",
        help = "Which images of the release to write (CPU is updated before FPGA)"
    )]
    pub(crate) target: Target,
    #[arg(
        long,
        requires = "version",
        help = "Download the release bundle again even if it is cached"
    )]
    pub(crate) force_download: bool,
    #[arg(
        long,
        conflicts_with = "version",
        help = "Treat <IMAGE> as an FPGA update image (`*-fpga-update.img`)"
    )]
    pub(crate) fpga: bool,
    #[arg(long, help = "Number of devices on the chain")]
    pub(crate) devices: usize,
    #[arg(long, help = "Network interface name (default: auto-detect)")]
    pub(crate) interface: Option<String>,
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        help = "How long opening the connection may take, enumeration included, e.g. 20s"
    )]
    pub(crate) open_timeout: Option<Duration>,
    #[arg(
        long,
        help = "Write and commit the image but do not reboot into it (it boots as a trial at the next reset)"
    )]
    pub(crate) no_activate: bool,
    #[arg(
        long,
        value_parser = humantime::parse_duration,
        default_value = "10s",
        help = "How long to wait after the CPU reboot before reconnecting"
    )]
    pub(crate) reboot_wait: Duration,
    #[arg(
        long,
        default_value_t = 5,
        help = "How many times to retry the reconnection after the reboot (2 s apart)"
    )]
    pub(crate) reconnect_attempts: u32,
    #[arg(
        long,
        help = "Only read the firmware versions (and the FPGA boot image); write nothing"
    )]
    pub(crate) verify_only: bool,
    #[arg(
        long,
        conflicts_with_all = ["verify_only", "no_activate", "fpga"],
        help = "Reboot into the new CPU image but do not confirm it (it rolls back at the next reset unless --confirm-only is run)"
    )]
    pub(crate) no_confirm: bool,
    #[arg(
        long,
        conflicts_with_all = ["verify_only", "fpga"],
        help = "Only confirm the CPU image that is currently running (after --no-confirm or a failed reconnection)"
    )]
    pub(crate) confirm_only: bool,
    #[arg(
        long,
        conflicts_with_all = ["image", "version", "fpga", "no_activate", "verify_only", "no_confirm", "confirm_only"],
        help = "Only reboot every device (software reset, no image is written); the devices come back unassigned and the tool does not reconnect"
    )]
    pub(crate) reboot_only: bool,
}
