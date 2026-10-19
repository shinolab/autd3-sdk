use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Subcommand;

use crate::util::{cargo_bin, cargo_build_args, run};

#[derive(Subcommand)]
pub enum ToolCmd {
    #[command(about = "Measure the communication performance")]
    Perftest {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(
            long,
            help = "Enable the `mem-profile` feature (allocation size histogram)"
        )]
        mem_profile: bool,
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            help = "Arguments forwarded to the tool"
        )]
        args: Vec<String>,
    },
    #[command(about = "Run the interactive firmware acceptance tests against a real device")]
    FirmwareTest {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            help = "Arguments forwarded to the tool"
        )]
        args: Vec<String>,
    },
    #[command(
        about = "Update the CPU / FPGA firmware over UDP (no J-Link / Vivado) and reboot the devices"
    )]
    FirmwareOta {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            help = "Arguments forwarded to the tool (the flash image path or `--version X.Y.Z` first)"
        )]
        args: Vec<String>,
    },
    #[command(about = "Read / write raw FPGA bus addresses over UDP (diagnostics only)")]
    FpgaMemory {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            help = "Arguments forwarded to the tool (`read <ADDR>` or `write <ADDR> <WORD>...` first)"
        )]
        args: Vec<String>,
    },
}

pub fn run_tool(root: &Path, cmd: ToolCmd) -> Result<()> {
    match cmd {
        ToolCmd::Perftest {
            debug,
            mem_profile,
            args,
        } => {
            let features: &[&str] = if mem_profile { &["mem-profile"] } else { &[] };
            let bin = build_bin(
                root,
                "autd3-rs-perftest",
                "autd3-rs-perftest",
                debug,
                features,
            )?;
            run(&bin.to_string_lossy(), &args, root)
        }
        ToolCmd::FirmwareTest { debug, args } => {
            let bin = build_bin(
                root,
                "autd3-rs-firmware-test",
                "autd3-rs-firmware-test",
                debug,
                &[],
            )?;
            run(&bin.to_string_lossy(), &args, root)
        }
        ToolCmd::FirmwareOta { debug, args } => {
            let bin = build_bin(
                root,
                "autd3-rs-firmware-ota",
                "autd3-firmware-ota",
                debug,
                &[],
            )?;
            run(&bin.to_string_lossy(), &args, root)
        }
        ToolCmd::FpgaMemory { debug, args } => {
            let bin = build_bin(
                root,
                "autd3-rs-fpga-memory",
                "autd3-fpga-memory",
                debug,
                &[],
            )?;
            run(&bin.to_string_lossy(), &args, root)
        }
    }
}

fn build_bin(root: &Path, pkg: &str, bin: &str, debug: bool, features: &[&str]) -> Result<PathBuf> {
    let mut build_args = cargo_build_args(pkg, None, debug);
    let features_arg = features.join(",");
    if !features.is_empty() {
        build_args.push("--features");
        build_args.push(&features_arg);
    }
    run("cargo", build_args, root)?;
    Ok(cargo_bin(root, None, debug, bin))
}
