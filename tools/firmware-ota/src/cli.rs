use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};

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
        The CPU image is written to the inactive slot, the devices reboot into it, and the tool \
        reconnects to confirm it (the reboot leaves the devices unassigned, so the chain is \
        enumerated again); an unconfirmed image rolls back at the next reset. The FPGA image is \
        written to the update slot and the FPGA reconfigures in place (golden image fallback)."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    #[command(about = "Write a local flash image; its version is not checked")]
    Local {
        #[arg(help = "Flash image: `autd3-cpu.bin` (or `*-fpga-update.img` with --fpga)")]
        image: PathBuf,
        #[arg(
            long,
            help = "Treat <IMAGE> as an FPGA update image (`*-fpga-update.img`)"
        )]
        fpga: bool,
        #[command(flatten)]
        update: UpdateArgs,
        #[command(flatten)]
        connection: Connection,
    },
    #[command(about = "Download a release bundle (0.10.0 or newer) and write it")]
    Release {
        #[arg(help = "Release to download and write (e.g. 0.10.0)")]
        version: String,
        #[arg(
            long,
            value_enum,
            default_value_t = Target::Both,
            help = "Which images of the release to write (CPU is updated before FPGA)"
        )]
        target: Target,
        #[arg(long, help = "Download the release bundle again even if it is cached")]
        force_download: bool,
        #[command(flatten)]
        update: UpdateArgs,
        #[command(flatten)]
        connection: Connection,
    },
    #[command(
        about = "Only read the CPU / FPGA firmware versions and the running images; write nothing"
    )]
    Verify {
        #[command(flatten)]
        connection: Connection,
    },
    #[command(
        about = "Only confirm the CPU image that is currently running (after --no-confirm or a failed reconnection)"
    )]
    Confirm {
        #[command(flatten)]
        connection: Connection,
    },
    #[command(
        about = "Only reboot every device (software reset); the devices come back unassigned and the tool does not reconnect"
    )]
    Reboot {
        #[command(flatten)]
        connection: Connection,
    },
}

#[derive(Args)]
pub(crate) struct Connection {
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
}

#[derive(Args)]
pub(crate) struct UpdateArgs {
    #[arg(
        long,
        help = "Write and commit the image but do not reboot into it (it boots as a trial at the next reset)"
    )]
    pub(crate) no_activate: bool,
    #[arg(
        long,
        conflicts_with = "no_activate",
        help = "Reboot into the new CPU image but do not confirm it (it rolls back at the next reset unless `confirm` is run)"
    )]
    pub(crate) no_confirm: bool,
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
}

impl Command {
    pub(crate) fn connection(&self) -> &Connection {
        match self {
            Command::Local { connection, .. }
            | Command::Release { connection, .. }
            | Command::Verify { connection }
            | Command::Confirm { connection }
            | Command::Reboot { connection } => connection,
        }
    }
}
