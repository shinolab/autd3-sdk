use std::path::Path;

use anyhow::Result;
use clap::{Args, Subcommand};

use crate::util::run_cargo;

#[derive(Args)]
pub(crate) struct ToolArgs {
    #[arg(long, help = "Build the dev profile instead of release")]
    debug: bool,
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        help = "Arguments forwarded to the tool"
    )]
    args: Vec<String>,
}

#[derive(Subcommand)]
pub(crate) enum ToolCmd {
    #[command(about = "Measure the communication performance")]
    Perftest(ToolArgs),
    #[command(about = "Run the interactive firmware acceptance tests against a real device")]
    FirmwareTest(ToolArgs),
    #[command(
        about = "Update the CPU / FPGA firmware over UDP (no J-Link / Vivado) and reboot the devices"
    )]
    FirmwareOta(ToolArgs),
    #[command(about = "Read / write raw FPGA bus addresses over UDP (diagnostics only)")]
    FpgaMemory(ToolArgs),
    #[command(about = "Run emulated devices on the loopback until stdin is closed")]
    UdpEmulator(ToolArgs),
}

pub(crate) fn run_tool(root: &Path, cmd: ToolCmd) -> Result<()> {
    let (pkg, bin, tool) = match cmd {
        ToolCmd::Perftest(tool) => ("autd3-rs-perftest", "autd3-rs-perftest", tool),
        ToolCmd::FirmwareTest(tool) => ("autd3-rs-firmware-test", "autd3-rs-firmware-test", tool),
        ToolCmd::FirmwareOta(tool) => ("autd3-rs-firmware-ota", "autd3-firmware-ota", tool),
        ToolCmd::FpgaMemory(tool) => ("autd3-rs-fpga-memory", "autd3-fpga-memory", tool),
        ToolCmd::UdpEmulator(tool) => ("autd3-rs-udp-emulator", "autd3-udp-emulator", tool),
    };
    let mut args = vec!["run", "-p", pkg, "--bin", bin];
    if !tool.debug {
        args.push("--release");
    }
    args.push("--");
    args.extend(tool.args.iter().map(String::as_str));
    run_cargo(args, root)
}
