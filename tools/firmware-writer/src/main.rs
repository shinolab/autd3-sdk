mod list;
mod util;
mod write;

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(
    name = "autd3-firmware-writer",
    about = "Flash AUTD3 CPU/FPGA firmware",
    long_about = "Download a released AUTD3 firmware bundle, or take local images, and write them to the device. \
                  CPU images (*.bin) are flashed via SEGGER J-Link and FPGA images (*.mcs) \
                  via Xilinx Vivado; both tools must be installed and on PATH."
)]
struct Cli {
    #[arg(long, conflicts_with_all = ["cpu", "fpga"])]
    version: Option<String>,

    #[arg(long, value_enum, conflicts_with_all = ["cpu", "fpga"])]
    target: Option<Target>,

    #[arg(long, conflicts_with_all = ["cpu", "fpga"])]
    force_download: bool,

    #[arg(long, conflicts_with_all = ["version", "cpu", "fpga"])]
    list: bool,

    #[arg(
        long,
        value_name = "BIN",
        help = "Write this local CPU image instead of a release"
    )]
    cpu: Option<PathBuf>,

    #[arg(
        long,
        value_name = "MCS",
        help = "Write this local FPGA image instead of a release"
    )]
    fpga: Option<PathBuf>,
}

#[derive(Copy, Clone, ValueEnum)]
pub enum Target {
    Both,
    Fpga,
    Cpu,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.list {
        return list::print_available_versions();
    }
    if cli.cpu.is_some() || cli.fpga.is_some() {
        return write::write_local(cli.cpu.as_deref(), cli.fpga.as_deref());
    }
    let Some(version) = cli.version.as_deref() else {
        bail!("--version, --cpu or --fpga is required (pass --list to see available versions)");
    };
    write::write(version, cli.target, cli.force_download)
}
