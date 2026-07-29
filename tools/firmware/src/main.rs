mod list;
mod series;
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
                  via Xilinx Vivado; both tools must be installed and on PATH. \
                  Pass --legacy to fetch the pre-rewrite firmware from shinolab/autd3-firmware \
                  instead; the images are flashed exactly the same way."
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

    /// Use the pre-rewrite release series (shinolab/autd3-firmware), e.g. v12.1.0
    #[arg(long)]
    legacy: bool,
}

#[derive(Copy, Clone, ValueEnum)]
pub enum Target {
    Both,
    Fpga,
    Cpu,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let series = series::Series::from_flag(cli.legacy);
    if cli.list {
        return list::print_available_versions(series);
    }
    let Some(version) = cli.version.as_deref() else {
        bail!("--version is required (pass --list to see available versions)");
    };
    write::write(version, cli.target, cli.force_download, series)
}
