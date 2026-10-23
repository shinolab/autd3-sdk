use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::{cargo_bin, cargo_build_args, run};

#[derive(Subcommand)]
pub(crate) enum ToolCmd {
    #[command(about = "Measure the communication performance")]
    Perftest {
        #[arg(long, help = "Build the dev profile instead of release")]
        debug: bool,
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

pub(crate) fn run_tool(root: &Path, cmd: ToolCmd) -> Result<()> {
    match cmd {
        ToolCmd::Perftest { debug, args } => {
            run_bin(root, "autd3-rs-perftest", "autd3-rs-perftest", debug, &args)
        }
        ToolCmd::FirmwareTest { debug, args } => run_bin(
            root,
            "autd3-rs-firmware-test",
            "autd3-rs-firmware-test",
            debug,
            &args,
        ),
        ToolCmd::FirmwareOta { debug, args } => run_bin(
            root,
            "autd3-rs-firmware-ota",
            "autd3-firmware-ota",
            debug,
            &args,
        ),
        ToolCmd::FpgaMemory { debug, args } => run_bin(
            root,
            "autd3-rs-fpga-memory",
            "autd3-fpga-memory",
            debug,
            &args,
        ),
    }
}

fn run_bin(root: &Path, pkg: &str, bin: &str, debug: bool, args: &[String]) -> Result<()> {
    run("cargo", cargo_build_args(pkg, None, debug), root)?;
    let bin = cargo_bin(root, None, debug, bin);
    run(&bin.to_string_lossy(), args, root)
}
