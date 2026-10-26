mod bump;
mod changelog;
mod check;
mod clean;
mod component;
mod console;
mod cpu;
mod cpu_codegen;
mod cs;
mod deadcode;
mod doc;
mod emulator;
mod ffi;
mod firmware;
mod fpga;
mod fpga_codegen;
mod holo_wgpu;
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
use check::{CheckCmd, run_check};
use clean::CleanArgs;
use console::{ConsoleCmd, run_console};
use cpu::{CpuCmd, run_cpu};
use cs::{CsCmd, run_cs};
use doc::{DocCmd, run_doc};
use emulator::{EmulatorCmd, run_emulator};
use ffi::{FfiCmd, run_ffi};
use firmware::{FirmwareCmd, run_firmware};
use fpga::{FpgaCmd, run_fpga};
use holo_wgpu::{HoloWgpuCmd, run_holo_wgpu};
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
    #[command(about = "The Rust client workspace (`crates/`)")]
    Rust {
        #[command(subcommand)]
        cmd: RustCmd,
    },
    #[command(about = "The CPU firmware (`firmware/cpu/`, no_std Rust)")]
    Cpu {
        #[command(subcommand)]
        cmd: CpuCmd,
    },
    #[command(about = "The auxiliary tools (`tools/`)")]
    Tool {
        #[command(subcommand)]
        cmd: ToolCmd,
    },
    #[command(about = "The wgpu holo backend (`extras/autd3-rs-pattern-holo-wgpu/`)")]
    HoloWgpu {
        #[command(subcommand)]
        cmd: HoloWgpuCmd,
    },
    #[command(about = "The sound-field simulator (`simulator/`)")]
    Simulator {
        #[command(subcommand)]
        cmd: SimulatorCmd,
    },
    #[command(about = "The GUI console (`console/`)")]
    Console {
        #[command(subcommand)]
        cmd: ConsoleCmd,
    },
    #[command(about = "The firmware emulator (`extras/autd3-rs-emulator/`)")]
    Emulator {
        #[command(subcommand)]
        cmd: EmulatorCmd,
    },
    #[command(about = "The FPGA firmware (`firmware/fpga/`, Vivado required)")]
    Fpga {
        #[command(subcommand)]
        cmd: FpgaCmd,
    },
    #[command(about = "The firmware distribution: bundle a release zip, write it to a device")]
    Firmware {
        #[command(subcommand)]
        cmd: FirmwareCmd,
    },
    #[command(about = "The Python bindings (`bindings/python/`)")]
    Py {
        #[command(subcommand)]
        cmd: PyCmd,
    },
    #[command(about = "The C ABI layer every binding is built on (`bindings/ffi/`)")]
    Ffi {
        #[command(subcommand)]
        cmd: FfiCmd,
    },
    #[command(about = "The C# bindings (`bindings/csharp/`)")]
    Cs {
        #[command(subcommand)]
        cmd: CsCmd,
    },
    #[command(about = "The Unity UPM packages (`bindings/unity/`)")]
    Unity {
        #[command(subcommand)]
        cmd: UnityCmd,
    },
    #[command(about = "The third-party license notices shipped with the distributed artifacts")]
    License {
        #[command(subcommand)]
        cmd: LicenseCmd,
    },
    #[command(about = "The documentation site (`doc/`)")]
    Doc {
        #[command(subcommand)]
        cmd: DocCmd,
    },
    #[command(
        about = "The soundness and mutation checkers (Miri, sanitizers, cargo-careful, cargo-mutants)."
    )]
    Check {
        #[command(subcommand)]
        cmd: CheckCmd,
    },
    #[command(about = "Generate CHANGELOG.md / release notes with git-cliff")]
    Changelog(ChangelogCmd),
    #[command(about = "Bump a component's version and regenerate CHANGELOG.md (no git operations)")]
    BumpVersion(BumpVersionCmd),
    #[command(about = "Remove every scope's build outputs (FPGA only with --fpga).")]
    Clean(TopCleanArgs),
}

#[derive(clap::Args)]
struct TopCleanArgs {
    #[command(flatten)]
    common: CleanArgs,
    #[arg(
        long,
        help = "Also remove the Vivado project and the FPGA build outputs (re-synthesis takes tens of minutes)"
    )]
    fpga: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = workspace_root();
    match cli.cmd {
        TopCmd::Rust { cmd } => run_rust(&root, &cmd),
        TopCmd::Cpu { cmd } => run_cpu(&root, &cmd),
        TopCmd::Tool { cmd } => run_tool(&root, cmd),
        TopCmd::HoloWgpu { cmd } => run_holo_wgpu(&root, &cmd),
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
        TopCmd::Check { cmd } => run_check(&root, &cmd),
        TopCmd::Changelog(cmd) => run_changelog(&root, &cmd),
        TopCmd::BumpVersion(cmd) => run_bump_version(&root, &cmd),
        TopCmd::Clean(args) => clean::scope(&root, args.common, |cleaner| {
            rust::clean(cleaner)?;
            cpu::clean(cleaner)?;
            holo_wgpu::clean(cleaner)?;
            simulator::clean(cleaner)?;
            console::clean(cleaner)?;
            emulator::clean(cleaner)?;
            firmware::clean(cleaner)?;
            py::clean(cleaner)?;
            ffi::clean(cleaner)?;
            cs::clean(cleaner)?;
            unity::clean(cleaner)?;
            doc::clean(cleaner)?;
            if args.fpga {
                fpga::clean(cleaner)?;
            } else {
                println!("kept the FPGA build outputs; pass --fpga to remove them too");
            }
            Ok(())
        }),
    }
}
