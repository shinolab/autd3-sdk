mod cli;
mod cpu;
mod error;
mod fpga;
mod fpga_image;
mod update;
mod verify;

use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;

use crate::cli::{Cli, Command, Connection};
use crate::cpu::{main_cpu, print_cpu};
use crate::error::OtaError;
use crate::fpga::{main_fpga, print_fpga};
use crate::update::{ACK_TIMEOUT, REBOOT_ACK_TIMEOUT};
use autd3_cpu_wire::update::RunningImage;
use autd3_firmware_writer::bundle;
use autd3_rs::commands::{Command as ClientCommand, Reboot, UpdateConfirm};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::{Client, Error, FirmwareVersion, Interface, TransportOption, Version};

fn read_image(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("reading {}", path.display()))
}

pub(crate) struct Connected {
    client: Arc<Client>,
    heartbeat: tokio::task::AbortHandle,
}

impl Deref for Connected {
    type Target = Client;

    fn deref(&self) -> &Client {
        &self.client
    }
}

impl Drop for Connected {
    fn drop(&mut self) {
        self.heartbeat.abort();
    }
}

impl Connected {
    pub(crate) async fn send_within<C: ClientCommand>(
        &self,
        timeout: Duration,
        cmd: C,
    ) -> Result<(), OtaError> {
        within(timeout, self.client.send(cmd)).await
    }

    pub(crate) async fn send<C: ClientCommand>(&self, cmd: C) -> Result<(), OtaError> {
        self.send_within(ACK_TIMEOUT, cmd).await
    }

    pub(crate) async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, OtaError> {
        within(ACK_TIMEOUT, self.client.read_firmware_version()).await
    }

    pub(crate) async fn read_running_image(&self) -> Result<Vec<Option<RunningImage>>, OtaError> {
        within(ACK_TIMEOUT, self.client.read_running_image()).await
    }
}

async fn within<T>(
    timeout: Duration,
    replied: impl Future<Output = Result<T, Error>>,
) -> Result<T, OtaError> {
    Ok(tokio::time::timeout(timeout, replied)
        .await
        .map_err(|_| OtaError::Timeout(timeout))??)
}

async fn connect(connection: &Connection, num_devices: usize) -> Result<Connected, Error> {
    let default = TransportOption::default();
    let option = TransportOption {
        iface: connection
            .interface
            .as_deref()
            .map_or(Interface::Auto, |name| Interface::Name(name.to_owned())),
        open_timeout: connection.open_timeout.unwrap_or(default.open_timeout),
        sync_on_open: false,
        ..default
    };
    let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()));
    let client = Arc::new(Client::open(&geometry, &option).await?);
    println!("{} device(s) on the bus", client.num_devices());
    let heartbeat = tokio::spawn({
        let client = Arc::clone(&client);
        let mut interval = tokio::time::interval(Duration::from_millis(10));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        async move {
            loop {
                interval.tick().await;
                if let Err(e) = client.send_heartbeat().await {
                    eprintln!("warning: sending a heartbeat failed: {e}");
                }
            }
        }
    })
    .abort_handle();
    Ok(Connected { client, heartbeat })
}

#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() -> Result<()> {
    let command = Cli::parse().command;
    match &command {
        Command::Reboot { connection } => {
            let client = connect(connection, connection.devices).await?;
            client.send_within(REBOOT_ACK_TIMEOUT, Reboot).await?;
            println!(
                "reboot acknowledged; the devices reset in about 100 ms and come back unassigned"
            );
            Ok(())
        }
        Command::Verify { connection } => {
            let client = connect(connection, connection.devices).await?;
            print_cpu(&client, "now").await?;
            print_fpga(&client, "now").await.map(drop)
        }
        Command::Confirm { connection } => {
            let client = connect(connection, connection.devices).await?;
            print_cpu(&client, "before confirm").await?;
            client.send(UpdateConfirm).await?;
            print_cpu(&client, "after confirm").await.map(drop)
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
