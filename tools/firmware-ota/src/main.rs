mod cli;
mod cpu;
mod fpga_image;
mod session;
mod udp;
mod update;
mod verify;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;

use crate::cli::{Cli, Target};
use crate::cpu::main_cpu;
use crate::fpga_image::FpgaFirmwareImage;
use crate::session::{Stage, open_and_run};
use crate::udp::connect;
use crate::update::reboot;
use autd3_firmware_writer::bundle;

async fn main_fpga(cli: &Cli, bytes: Vec<u8>, invalid_image_hint: &'static str) -> Result<()> {
    if cli.verify_only {
        open_and_run(cli, cli.devices, Stage::FpgaVerify).await?;
        return Ok(());
    }
    let image = FpgaFirmwareImage::from_update_bin(bytes).context(invalid_image_hint)?;
    println!(
        "FPGA update image: {} bytes, crc32 {:#010x}",
        image.len(),
        image.crc32()
    );
    open_and_run(
        cli,
        cli.devices,
        Stage::FpgaUpdate {
            image: &image,
            activate: !cli.no_activate,
        },
    )
    .await?;
    Ok(())
}

fn read_image(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("reading {}", path.display()))
}

#[derive(Debug, PartialEq, Eq)]
struct ReleaseImages {
    cpu: Option<PathBuf>,
    fpga: Option<PathBuf>,
}

fn release_images(bundle: bundle::Bundle, target: Target) -> Result<ReleaseImages> {
    let cpu = if target.cpu() {
        Some(
            bundle
                .cpu
                .context("the release bundle has no CPU firmware image (*.bin)")?,
        )
    } else {
        None
    };
    let fpga = if target.fpga() {
        Some(
            bundle
                .fpga_update
                .context("the release bundle has no FPGA update image (*-fpga-update.img)")?,
        )
    } else {
        None
    };
    Ok(ReleaseImages { cpu, fpga })
}

#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.devices == 0 {
        bail!("--devices must be at least 1");
    }
    if cli.reboot_only {
        let client = connect(cli.interface.as_deref(), cli.open_timeout, cli.devices).await?;
        println!("{} device(s) on the bus", client.num_devices());
        reboot(&client).await?;
        println!("reboot acknowledged; the devices reset in about 100 ms and come back unassigned");
        return Ok(());
    }
    match (&cli.image, &cli.version) {
        (Some(path), _) => {
            let bytes = read_image(path)?;
            if cli.fpga {
                main_fpga(
                    &cli,
                    bytes,
                    "pass the `autd3-fpga-update.img` produced by `cargo xtask fpga build` \
                     (not the golden image; a golden image must never be written to the update slot)",
                )
                .await
            } else {
                main_cpu(&cli, &bytes, None).await
            }
        }
        (None, Some(version)) => {
            if cli.confirm_only && cli.target != Target::Cpu {
                bail!("--confirm-only only applies to the CPU image; pass --target cpu");
            }
            let release = bundle::parse_release(version)?;
            let bundle = bundle::fetch(release, cli.force_download)?;
            let images = release_images(bundle, cli.target)?;
            let cpu = images.cpu.as_deref().map(read_image).transpose()?;
            let fpga = images.fpga.as_deref().map(read_image).transpose()?;
            let shown = version.trim_start_matches('v');
            if let Some(bytes) = cpu {
                println!("== CPU firmware v{shown} ==");
                main_cpu(&cli, &bytes, Some(release)).await?;
            }
            if let Some(bytes) = fpga {
                println!("== FPGA firmware v{shown} ==");
                main_fpga(
                    &cli,
                    bytes,
                    "the FPGA update image in the release bundle is not usable; retry with `--force-download`",
                )
                .await?;
            }
            Ok(())
        }
        (None, None) => bail!("pass an image file or --version"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_images_of_the_target_are_required_from_the_bundle() {
        let cpu = PathBuf::from("autd3-cpu.bin");
        let fpga = PathBuf::from("autd3-fpga-update.img");
        let cpu_only = bundle::Bundle {
            cpu: Some(cpu.clone()),
            ..bundle::Bundle::default()
        };
        let fpga_only = bundle::Bundle {
            fpga_update: Some(fpga.clone()),
            ..bundle::Bundle::default()
        };
        assert_eq!(
            release_images(cpu_only.clone(), Target::Cpu).unwrap(),
            ReleaseImages {
                cpu: Some(cpu),
                fpga: None
            }
        );
        assert_eq!(
            release_images(fpga_only.clone(), Target::Fpga).unwrap(),
            ReleaseImages {
                cpu: None,
                fpga: Some(fpga)
            }
        );
        for (bundle, target) in [
            (cpu_only.clone(), Target::Both),
            (cpu_only, Target::Fpga),
            (fpga_only.clone(), Target::Both),
            (fpga_only, Target::Cpu),
        ] {
            assert!(release_images(bundle, target).is_err(), "{target:?}");
        }
    }
}
