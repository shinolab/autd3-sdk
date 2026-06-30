use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use crate::util::{run, run_built_bin};

#[derive(Subcommand)]
pub enum FirmwareCmd {
    Write(WriteArgs),
}

#[derive(clap::Args)]
pub struct WriteArgs {
    #[arg(long)]
    version: Option<String>,

    #[arg(long)]
    target: Option<String>,

    #[arg(long)]
    force_download: bool,

    #[arg(long)]
    list: bool,
}

pub fn run_firmware(root: &Path, cmd: FirmwareCmd) -> Result<()> {
    match cmd {
        FirmwareCmd::Write(args) => write(root, &args),
    }
}

fn write(root: &Path, args: &WriteArgs) -> Result<()> {
    run(
        "cargo",
        ["build", "-p", "autd3-firmware", "--release"],
        root,
    )?;
    let bin = root
        .join("target")
        .join("release")
        .join(if cfg!(windows) {
            "autd3-firmware.exe"
        } else {
            "autd3-firmware"
        });

    let mut cli_args: Vec<String> = Vec::new();
    if let Some(version) = &args.version {
        cli_args.push("--version".to_string());
        cli_args.push(version.clone());
    }
    if let Some(target) = &args.target {
        cli_args.push("--target".to_string());
        cli_args.push(target.clone());
    }
    if args.force_download {
        cli_args.push("--force-download".to_string());
    }
    if args.list {
        cli_args.push("--list".to_string());
    }

    run_built_bin(&bin, &cli_args, true, root)
}
