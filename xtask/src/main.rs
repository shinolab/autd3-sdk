mod bump;
mod changelog;
mod component;
mod console;
mod cpu;
mod cs;
mod emulator;
mod example;
mod ffi;
mod firmware;
mod fpga;
mod license;
mod py;
mod rust;
mod simulator;
mod tool;
mod util;
mod vendor;

use anyhow::Result;
use clap::{Parser, Subcommand};

use bump::{BumpVersionCmd, run_bump_version};
use changelog::{ChangelogCmd, run_changelog};
use console::{ConsoleCmd, run_console};
use cpu::{CpuCmd, run_cpu};
use cs::{CsCmd, run_cs};
use emulator::{EmulatorCmd, run_emulator};
use example::{ExampleCmd, run_example};
use ffi::{FfiCmd, run_ffi};
use firmware::{FirmwareCmd, run_firmware};
use fpga::{FpgaCmd, run_fpga};
use license::{LicenseCmd, run_license};
use py::{PyCmd, run_py};
use rust::{RustCmd, run_rust};
use simulator::{SimulatorCmd, run_simulator};
use tool::{ToolCmd, run_tool};
use util::workspace_root;
use vendor::{VendorFwCmd, run_vendor_fw};

#[derive(Parser)]
#[command(name = "xtask", about = "autd3-rs dev task runner")]
struct Cli {
    #[command(subcommand)]
    cmd: TopCmd,
}

#[derive(Subcommand)]
enum TopCmd {
    Rust {
        #[command(subcommand)]
        cmd: RustCmd,
    },
    Cpu {
        #[command(subcommand)]
        cmd: CpuCmd,
    },
    Tool {
        #[command(subcommand)]
        cmd: ToolCmd,
    },
    Simulator {
        #[command(subcommand)]
        cmd: SimulatorCmd,
    },
    Console {
        #[command(subcommand)]
        cmd: ConsoleCmd,
    },
    Emulator {
        #[command(subcommand)]
        cmd: EmulatorCmd,
    },
    Fpga {
        #[command(subcommand)]
        cmd: FpgaCmd,
    },
    Firmware {
        #[command(subcommand)]
        cmd: FirmwareCmd,
    },
    Py {
        #[command(subcommand)]
        cmd: PyCmd,
    },
    Ffi {
        #[command(subcommand)]
        cmd: FfiCmd,
    },
    Cs {
        #[command(subcommand)]
        cmd: CsCmd,
    },
    License {
        #[command(subcommand)]
        cmd: LicenseCmd,
    },
    Example(ExampleCmd),
    Changelog(ChangelogCmd),
    BumpVersion(BumpVersionCmd),
    VendorFw(VendorFwCmd),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = workspace_root();
    match cli.cmd {
        TopCmd::Rust { cmd } => run_rust(&root, &cmd),
        TopCmd::Cpu { cmd } => run_cpu(&root, &cmd),
        TopCmd::Tool { cmd } => run_tool(&root, cmd),
        TopCmd::Simulator { cmd } => run_simulator(&root, cmd),
        TopCmd::Console { cmd } => run_console(&root, cmd),
        TopCmd::Emulator { cmd } => run_emulator(&root, &cmd),
        TopCmd::Fpga { cmd } => run_fpga(&root, &cmd),
        TopCmd::Firmware { cmd } => run_firmware(&root, cmd),
        TopCmd::Py { cmd } => run_py(&root, cmd),
        TopCmd::Ffi { cmd } => run_ffi(&root, cmd),
        TopCmd::Cs { cmd } => run_cs(&root, cmd),
        TopCmd::License { cmd } => run_license(&root, &cmd),
        TopCmd::Example(cmd) => run_example(&root, &cmd),
        TopCmd::Changelog(cmd) => run_changelog(&root, &cmd),
        TopCmd::BumpVersion(cmd) => run_bump_version(&root, &cmd),
        TopCmd::VendorFw(cmd) => run_vendor_fw(&root, &cmd),
    }
}
