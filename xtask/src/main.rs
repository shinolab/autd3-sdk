mod bump;
mod changelog;
mod component;
mod console;
mod cpu;
mod cpu_codegen;
mod cs;
mod doc;
mod emulator;
mod ffi;
mod firmware;
mod fpga;
mod fpga_codegen;
mod license;
mod py;
mod rust;
mod simulator;
mod tool;
mod unity;
mod util;

use anyhow::Result;
use clap::{Parser, Subcommand};

use bump::{BumpVersionCmd, run_bump_version};
use changelog::{ChangelogCmd, run_changelog};
use console::{ConsoleCmd, run_console};
use cpu::{CpuCmd, run_cpu};
use cs::{CsCmd, run_cs};
use doc::{DocCmd, run_doc};
use emulator::{EmulatorCmd, run_emulator};
use ffi::{FfiCmd, run_ffi};
use firmware::{FirmwareCmd, run_firmware};
use fpga::{FpgaCmd, run_fpga};
use license::{LicenseCmd, run_license};
use py::{PyCmd, run_py};
use rust::{RustCmd, run_rust};
use simulator::{SimulatorCmd, run_simulator};
use tool::{ToolCmd, run_tool};
use unity::{UnityCmd, run_unity};
use util::workspace_root;

#[derive(Parser)]
#[command(name = "xtask", about = "autd3-sdk dev task runner")]
struct Cli {
    #[command(subcommand)]
    cmd: TopCmd,
}

#[derive(Subcommand)]
enum TopCmd {
    /// The Rust client workspace (`crates/`).
    Rust {
        #[command(subcommand)]
        cmd: RustCmd,
    },
    /// The CPU firmware (`firmware/cpu/`, no_std Rust).
    Cpu {
        #[command(subcommand)]
        cmd: CpuCmd,
    },
    /// The auxiliary tools (`tools/`).
    Tool {
        #[command(subcommand)]
        cmd: ToolCmd,
    },
    /// The sound-field simulator (`simulator/`).
    Simulator {
        #[command(subcommand)]
        cmd: SimulatorCmd,
    },
    /// The GUI console (`console/`).
    Console {
        #[command(subcommand)]
        cmd: ConsoleCmd,
    },
    /// The firmware emulator (`emulator/`).
    Emulator {
        #[command(subcommand)]
        cmd: EmulatorCmd,
    },
    /// The FPGA firmware (`firmware/fpga/`, Vivado required).
    Fpga {
        #[command(subcommand)]
        cmd: FpgaCmd,
    },
    /// The firmware distribution: bundle a release zip, write it to a device.
    Firmware {
        #[command(subcommand)]
        cmd: FirmwareCmd,
    },
    /// The Python bindings (`bindings/python/`).
    Py {
        #[command(subcommand)]
        cmd: PyCmd,
    },
    /// The C ABI layer every binding is built on (`bindings/ffi/`).
    Ffi {
        #[command(subcommand)]
        cmd: FfiCmd,
    },
    /// The C# bindings (`bindings/csharp/`).
    Cs {
        #[command(subcommand)]
        cmd: CsCmd,
    },
    /// The Unity UPM packages (`bindings/unity/`).
    Unity {
        #[command(subcommand)]
        cmd: UnityCmd,
    },
    /// The third-party license notices shipped with the distributed artifacts.
    License {
        #[command(subcommand)]
        cmd: LicenseCmd,
    },
    /// The documentation site (`doc/`).
    Doc {
        #[command(subcommand)]
        cmd: DocCmd,
    },
    /// Generate CHANGELOG.md / release notes with git-cliff.
    Changelog(ChangelogCmd),
    /// Bump a component's version and regenerate CHANGELOG.md (no git operations).
    BumpVersion(BumpVersionCmd),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = workspace_root();
    match cli.cmd {
        TopCmd::Rust { cmd } => run_rust(&root, &cmd),
        TopCmd::Cpu { cmd } => run_cpu(&root, &cmd),
        TopCmd::Tool { cmd } => run_tool(&root, cmd),
        TopCmd::Simulator { cmd } => run_simulator(&root, &cmd),
        TopCmd::Console { cmd } => run_console(&root, &cmd),
        TopCmd::Emulator { cmd } => run_emulator(&root, &cmd),
        TopCmd::Fpga { cmd } => run_fpga(&root, &cmd),
        TopCmd::Firmware { cmd } => run_firmware(&root, cmd),
        TopCmd::Py { cmd } => run_py(&root, cmd),
        TopCmd::Ffi { cmd } => run_ffi(&root, &cmd),
        TopCmd::Cs { cmd } => run_cs(&root, cmd),
        TopCmd::Unity { cmd } => run_unity(&root, cmd),
        TopCmd::License { cmd } => run_license(&root, &cmd),
        TopCmd::Doc { cmd } => run_doc(&root, &cmd),
        TopCmd::Changelog(cmd) => run_changelog(&root, &cmd),
        TopCmd::BumpVersion(cmd) => run_bump_version(&root, &cmd),
    }
}
