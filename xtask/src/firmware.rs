use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;

use crate::util::{run, run_built_bin};

#[derive(Subcommand)]
pub enum FirmwareCmd {
    /// Download a released firmware bundle and write it to the device (J-Link / Vivado)
    Write(WriteArgs),
    /// Build the CPU and FPGA firmware and zip them into `firmware/dist/`
    Bundle(BundleArgs),
}

#[derive(clap::Args)]
pub struct BundleArgs {
    /// Re-synthesize the FPGA even when a bitstream already exists
    #[arg(long)]
    force: bool,
}

#[derive(clap::Args)]
pub struct WriteArgs {
    /// Firmware version to write (e.g. 1.2.3)
    #[arg(long)]
    version: Option<String>,
    /// What to write: `both` (default), `cpu`, or `fpga`
    #[arg(long)]
    target: Option<String>,
    /// Re-download the bundle even when it is already cached
    #[arg(long)]
    force_download: bool,
    /// List the available firmware versions and exit
    #[arg(long)]
    list: bool,
}

pub fn run_firmware(root: &Path, cmd: FirmwareCmd) -> Result<()> {
    match cmd {
        FirmwareCmd::Write(args) => write(root, &args),
        FirmwareCmd::Bundle(args) => bundle(root, &args),
    }
}

fn bundle(root: &Path, args: &BundleArgs) -> Result<()> {
    crate::bump::firmware_series(root)?;
    let component = crate::component::COMPONENTS
        .iter()
        .find(|c| c.name == "firmware")
        .context("no `firmware` component")?;
    let version = component.current_version(root)?;

    let cpu = crate::cpu::cpu_build(root)?;
    let fpga = crate::fpga::fpga_build(root, args.force)?;

    let stem = format!("autd3-sdk-firmware-v{version}");
    let dist = root.join("firmware/dist");
    std::fs::create_dir_all(&dist).with_context(|| format!("creating {}", dist.display()))?;
    let archive = dist.join(format!("{stem}.zip"));
    write_zip(
        &archive,
        &[(format!("{stem}.bin"), cpu), (format!("{stem}.mcs"), fpga)],
    )?;

    println!("firmware bundle: {}", archive.display());
    Ok(())
}

fn write_zip(archive: &Path, entries: &[(String, PathBuf)]) -> Result<()> {
    let file = std::fs::File::create(archive)
        .with_context(|| format!("creating {}", archive.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, path) in entries {
        zip.start_file(name.clone(), options)?;
        let mut src =
            std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
        std::io::copy(&mut src, &mut zip).with_context(|| format!("adding {name}"))?;
    }
    zip.finish()?;
    Ok(())
}

fn write(root: &Path, args: &WriteArgs) -> Result<()> {
    run(
        "cargo",
        ["build", "-p", "autd3-firmware", "--release"],
        root,
    )?;
    let bin = root.join("target").join("release").join(if cfg!(windows) {
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
