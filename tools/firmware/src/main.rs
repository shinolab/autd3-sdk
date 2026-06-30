mod list;
mod util;
mod write;

use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(
    name = "autd3-firmware",
    about = "Flash AUTD3 CPU/FPGA firmware",
    long_about = "Download a released AUTD3 firmware bundle and write it to the device. \
                  CPU images (*.bin) are flashed via SEGGER J-Link and FPGA images (*.mcs) \
                  via Xilinx Vivado; both tools must be installed and on PATH."
)]
struct Cli {
    #[arg(long)]
    version: Option<String>,

    #[arg(long, value_enum)]
    target: Option<Target>,

    #[arg(long)]
    force_download: bool,

    #[arg(long)]
    list: bool,
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
    let Some(version) = cli.version.as_deref() else {
        bail!("--version is required (pass --list to see available versions)");
    };
    write::write(version, cli.target, cli.force_download)
}
