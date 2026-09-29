use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::{RUN_CAPABILITIES, run, run_built_bin_with};

const FIRMWARE_OTA_CAPABILITIES: &str = "cap_net_raw,cap_sys_nice+ep";

#[derive(Subcommand)]
pub enum ToolCmd {
    /// Measure the communication performance
    Perftest {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Do not wrap the run in `sudo`
        #[arg(long)]
        no_sudo: bool,
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
        /// Do not wrap the run in `sudo`
        #[arg(long)]
        no_sudo: bool,
        /// Arguments forwarded to the tool
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Update the CPU / FPGA firmware over UDP (no J-Link / Vivado) and reboot the devices
    FirmwareOta {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
        /// Do not wrap the run in `sudo`
        #[arg(long)]
        no_sudo: bool,
        /// Arguments forwarded to the tool (the flash image path or `--version X.Y.Z` first)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

pub fn run_tool(root: &Path, cmd: ToolCmd) -> Result<()> {
    match cmd {
        ToolCmd::Perftest {
            debug,
            no_sudo,
            mem_profile,
            args,
        } => {
            let features: &[&str] = if mem_profile { &["mem-profile"] } else { &[] };
            run_bin(
                root,
                "autd3-rs-perftest",
                debug,
                no_sudo,
                features,
                &args,
                RUN_CAPABILITIES,
            )
        }
        ToolCmd::FirmwareTest {
            debug,
            no_sudo,
            args,
        } => run_bin(
            root,
            "autd3-rs-firmware-test",
            debug,
            no_sudo,
            &[],
            &args,
            RUN_CAPABILITIES,
        ),
        ToolCmd::FirmwareOta {
            debug,
            no_sudo,
            args,
        } => run_bin(
            root,
            "autd3-rs-firmware-ota",
            debug,
            no_sudo,
            &[],
            &args,
            FIRMWARE_OTA_CAPABILITIES,
        ),
    }
}

fn run_bin(
    root: &Path,
    pkg: &str,
    debug: bool,
    no_sudo: bool,
    features: &[&str],
    args: &[String],
    capabilities: &str,
) -> Result<()> {
    let mut build_args: Vec<&str> = vec!["build", "-p", pkg];
    if !debug {
        build_args.push("--release");
    }
    let features_arg = features.join(",");
    if !features.is_empty() {
        build_args.push("--features");
        build_args.push(&features_arg);
    }
    run("cargo", build_args, root)?;

    let profile = if debug { "debug" } else { "release" };
    let bin = root.join("target").join(profile).join(pkg);
    run_built_bin_with(&bin, args, no_sudo, root, capabilities)
}
