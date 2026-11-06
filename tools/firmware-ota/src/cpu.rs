use std::time::Duration;

use anyhow::{Context, Result};

use autd3_cpu_wire::update::RunningImage;
use autd3_firmware_writer::image::CpuFirmwareImage;
use autd3_rs::Version;
use autd3_rs::commands::{Reboot, UpdateConfirm};

use crate::cli::{Connection, UpdateArgs};
use crate::error::OtaError;
use crate::error::{NotOnTrial, TrialNotRunning};
use crate::update::{REBOOT_ACK_TIMEOUT, activate_unrebooted, update_cpu};
use crate::verify::{Expect, check_expectation, check_trial, describe_running_image};
use crate::{Connected, connect};

const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const CONFIRM_HINT: &str = "the new CPU image is running but not confirmed: run `autd3-firmware-ota \
     confirm` (or simply run the update again) before the next power cycle, otherwise \
     the previous image boots again";
const UNACKNOWLEDGED_HINT: &str = "the activation was not acknowledged by every device: the devices \
     that received it run the new CPU image unconfirmed, the others still run the previous image \
     and boot the new one at their next power cycle. Once every device is reachable, check which \
     image runs with `verify` and confirm it with `confirm` (or simply run the update again)";

pub(crate) async fn main_cpu(
    connection: &Connection,
    update: &UpdateArgs,
    bytes: &[u8],
    release: Option<Version>,
) -> Result<()> {
    let image = CpuFirmwareImage::from_flash_image(bytes)?;
    println!(
        "image: {} bytes, crc32 {:#010x}",
        image.len(),
        image.crc32()
    );
    let client = connect(connection, connection.devices).await?;
    let devices = client.num_devices();
    let (before, _) = print_cpu(&client, "before").await?;
    update_cpu(&client, &image).await?;
    println!("image committed on every device");
    if update.no_activate {
        println!(
            "not activated (--no-activate); the committed image boots as a trial at the next \
             reset or power cycle. Run `confirm` once it is up, or it rolls back at the \
             reset after that"
        );
        return Ok(());
    }
    let acknowledged = match client.send_within(REBOOT_ACK_TIMEOUT, Reboot).await {
        Ok(()) => {
            println!("activation acknowledged; the devices reboot in about 100 ms");
            true
        }
        Err(OtaError::Timeout(_) | OtaError::Autd3(autd3_rs::Error::DeviceLost { .. })) => {
            eprintln!(
                "warning: not every device acknowledged the activation; the devices that \
                 have not rebooted are activated after the reconnection"
            );
            false
        }
        Err(e) => return Err(e.into()),
    };
    drop(client);

    let wait = update.reboot_wait;
    let attempts = update.reconnect_attempts.max(1);
    wait_for_reboot(wait).await;
    if !acknowledged {
        let activated_late = retry_reconnect(attempts, UNACKNOWLEDGED_HINT, async || {
            let client = connect(connection, devices).await?;
            Ok(activate_unrebooted(&client).await?)
        })
        .await?;
        if !activated_late.is_empty() {
            for device in &activated_late {
                println!(
                    "device {device} had not rebooted; activated now, it reboots in about 100 ms"
                );
            }
            wait_for_reboot(wait).await;
        }
    }
    let expect = release.map_or(Expect::ChangedFrom(&before), Expect::Exactly);
    retry_reconnect(attempts, CONFIRM_HINT, async || {
        reconnect(connection, devices, !update.no_confirm, expect).await
    })
    .await?;
    if update.no_confirm {
        println!("not confirmed (--no-confirm); {CONFIRM_HINT}");
        return Ok(());
    }
    println!("Ok!");
    Ok(())
}

async fn wait_for_reboot(wait: Duration) {
    println!("waiting {wait:?} for the devices to reboot before enumerating them again");
    tokio::time::sleep(wait).await;
}

async fn retry_reconnect<T>(
    attempts: u32,
    hint: &str,
    mut reconnect: impl AsyncFnMut() -> Result<T>,
) -> Result<T> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match reconnect().await {
            Ok(value) => return Ok(value),
            Err(e) if e.is::<TrialNotRunning>() || e.is::<NotOnTrial>() => return Err(e),
            Err(e) if attempt < attempts => {
                eprintln!(
                    "reconnection attempt {attempt}/{attempts} failed: {e:#}; retrying in {RECONNECT_INTERVAL:?}"
                );
                tokio::time::sleep(RECONNECT_INTERVAL).await;
            }
            Err(e) => {
                return Err(e).context(format!(
                    "reconnection failed {attempts} time(s) after the reboot; {hint}"
                ));
            }
        }
    }
}

async fn reconnect(
    connection: &Connection,
    devices: usize,
    confirm: bool,
    expect: Expect<'_>,
) -> Result<()> {
    let client = connect(connection, devices).await?;
    let (after, images) =
        print_cpu(&client, if confirm { "before confirm" } else { "now" }).await?;
    check_expectation(&after, expect)?;
    check_trial(&images, &after, expect)?;
    if confirm {
        client.send(UpdateConfirm).await?;
        println!("running image confirmed on every device");
        print_cpu(&client, "after confirm").await?;
    }
    Ok(())
}

pub(crate) async fn print_cpu(
    client: &Connected,
    label: &str,
) -> Result<(Vec<Version>, Vec<Option<RunningImage>>)> {
    let versions: Vec<Version> = client
        .read_firmware_version()
        .await?
        .iter()
        .map(|info| info.cpu)
        .collect();
    let images = client.read_running_image().await?;
    for (device, (version, &image)) in versions.iter().zip(&images).enumerate() {
        println!(
            "device {device}: CPU firmware {label} = {version} ({})",
            describe_running_image(image)
        );
    }
    Ok((versions, images))
}
