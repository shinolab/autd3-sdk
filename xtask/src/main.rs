mod bump;
mod changelog;
mod component;
mod cpu;
mod example;
mod ffi;
mod firmware;
mod fpga;
mod py;
mod rust;
mod tool;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};

use bump::{BumpVersionCmd, run_bump_version};
use changelog::{ChangelogCmd, run_changelog};
use cpu::{CpuCmd, run_cpu};
use example::{ExampleCmd, run_example};
use ffi::{FfiCmd, run_ffi};
use firmware::{FirmwareCmd, run_firmware};
use fpga::{FpgaCmd, run_fpga};
use py::{PyCmd, run_py};
use rust::{RustCmd, run_rust};
use tool::{ToolCmd, run_tool};
use util::workspace_root;

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
    Example(ExampleCmd),
    Changelog(ChangelogCmd),
    BumpVersion(BumpVersionCmd),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = workspace_root();
    match cli.cmd {
        TopCmd::Rust { cmd } => run_rust(&root, &cmd),
        TopCmd::Cpu { cmd } => run_cpu(&root, &cmd),
        TopCmd::Tool { cmd } => run_tool(&root, cmd),
        TopCmd::Fpga { cmd } => run_fpga(&root, &cmd),
        TopCmd::Firmware { cmd } => run_firmware(&root, cmd),
        TopCmd::Py { cmd } => run_py(&root, cmd),
        TopCmd::Ffi { cmd } => run_ffi(&root, cmd),
        TopCmd::Example(cmd) => run_example(&root, &cmd),
        TopCmd::Changelog(cmd) => run_changelog(&root, &cmd),
        TopCmd::BumpVersion(cmd) => run_bump_version(&root, &cmd),
    }
}
