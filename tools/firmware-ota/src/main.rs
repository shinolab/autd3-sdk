mod cli;
mod cpu;
mod error;
mod fpga;
mod fpga_image;
mod update;
mod verify;

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;

use crate::cli::{Cli, Command, Connection};
use crate::cpu::{check_cpu, main_cpu};
use crate::fpga::{main_fpga, print_fpga};
use crate::update::ACK_TIMEOUT;
use autd3_firmware_writer::bundle;
use autd3_rs::commands::Reboot;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, Error, Interface, TransportOption, Version};

fn read_image(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("reading {}", path.display()))
}

async fn connect(connection: &Connection, num_devices: usize) -> Result<Client, Error> {
    let default = TransportOption::default();
    let option = TransportOption {
        iface: connection
            .interface
            .as_deref()
            .map_or(Interface::Auto, |name| Interface::Name(name.to_owned())),
        open_timeout: connection.open_timeout.unwrap_or(default.open_timeout),
        ack_timeout: ACK_TIMEOUT,
        sync_on_open: false,
        heartbeat_interval: Some(Duration::from_millis(10)),
        ..default
    };
    let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()));
    let client = Client::open(&geometry, &option).await?;
    println!("{} device(s) on the bus", client.num_devices());
    Ok(client)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() -> Result<()> {
    let command = Cli::parse().command;
    match &command {
        Command::Reboot { connection } => {
            let client = connect(connection, connection.devices).await?;
            client.set_ack_timeout(Duration::from_secs(5));
            client.send(Reboot).await?;
            println!(
                "reboot acknowledged; the devices reset in about 100 ms and come back unassigned"
            );
            Ok(())
        }
        Command::Verify { connection } => {
            let client = connect(connection, connection.devices).await?;
            check_cpu(&client, false, None).await?;
            print_fpga(&client, "now").await.map(drop)
        }
        Command::Confirm { connection } => {
            let client = connect(connection, connection.devices).await?;
            check_cpu(&client, true, None).await
        }
        Command::Local {
            image,
            fpga,
            update,
            connection,
        } => {
            if *fpga && update.no_confirm {
                bail!("--no-confirm only applies to the CPU image");
            }
            let bytes = read_image(image)?;
            if *fpga {
                main_fpga(
                    connection,
                    update,
                    bytes,
                    "pass the `autd3-fpga-update.img` produced by `cargo xtask fpga build` \
                     (not the golden image; a golden image must never be written to the update slot)",
                )
                .await
            } else {
                main_cpu(connection, update, &bytes, None).await
            }
        }
        Command::Release {
            version,
            target,
            force_download,
            update,
            connection,
        } => {
            let release = bundle::parse_release(version)?;
            let bundle = bundle::fetch(release, *force_download)?;
            let cpu_path = target
                .cpu()
                .then(|| {
                    bundle
                        .cpu
                        .context("the release bundle has no CPU firmware image (*.bin)")
                })
                .transpose()?;
            let fpga_path = target
                .fpga()
                .then(|| {
                    bundle
                        .fpga_update
                        .context("the release bundle has no FPGA update image (*-fpga-update.img)")
                })
                .transpose()?;
            let cpu = cpu_path.as_deref().map(read_image).transpose()?;
            let fpga = fpga_path.as_deref().map(read_image).transpose()?;
            let shown = version.trim_start_matches('v');
            if let Some(bytes) = cpu {
                println!("== CPU firmware v{shown} ==");
                let release = Version {
                    major: release[0],
                    minor: release[1],
                    patch: release[2],
                };
                main_cpu(connection, update, &bytes, Some(release)).await?;
            }
            if let Some(bytes) = fpga {
                println!("== FPGA firmware v{shown} ==");
                main_fpga(
                    connection,
                    update,
                    bytes,
                    "the FPGA update image in the release bundle is not usable; retry with `--force-download`",
                )
                .await?;
            }
            Ok(())
        }
    }
}
