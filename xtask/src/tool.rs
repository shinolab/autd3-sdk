use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Subcommand;

use crate::util::{cargo_bin, cargo_build_args, run};

#[derive(Subcommand)]
pub enum ToolCmd {
    /// Measure the communication performance
    Perftest {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Enable the `mem-profile` feature (allocation size histogram)
        #[arg(long)]
        mem_profile: bool,
        /// Arguments forwarded to the tool
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run the interactive firmware acceptance tests against a real device
    FirmwareTest {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Arguments forwarded to the tool
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Update the CPU / FPGA firmware over UDP (no J-Link / Vivado) and reboot the devices
    FirmwareOta {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Arguments forwarded to the tool (the flash image path or `--version X.Y.Z` first)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
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
            let bin = build_bin(root, "autd3-rs-perftest", debug, features)?;
            run(&bin.to_string_lossy(), &args, root)
        }
        ToolCmd::FirmwareTest { debug, args } => {
            let bin = build_bin(root, "autd3-rs-firmware-test", debug, &[])?;
            run(&bin.to_string_lossy(), &args, root)
        }
        ToolCmd::FirmwareOta { debug, args } => {
            let bin = build_bin(root, "autd3-rs-firmware-ota", debug, &[])?;
            run(&bin.to_string_lossy(), &args, root)
        }
    }
}

fn build_bin(root: &Path, pkg: &str, debug: bool, features: &[&str]) -> Result<PathBuf> {
    let mut build_args = cargo_build_args(pkg, None, debug);
    let features_arg = features.join(",");
    if !features.is_empty() {
        build_args.push("--features");
        build_args.push(&features_arg);
    }
    run("cargo", build_args, root)?;
    Ok(cargo_bin(root, None, debug, pkg))
}
