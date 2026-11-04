use std::time::Duration;

use anyhow::{Context, Result};

use autd3_firmware_writer::image::CpuFirmwareImage;
use autd3_rs::commands::{Reboot, UpdateConfirm};
use autd3_rs::{Client, Version};

use crate::cli::{Connection, UpdateArgs};
use crate::connect;
use crate::error::{NotOnTrial, TrialNotRunning};
use crate::update::{activate_unrebooted, update_cpu};
use crate::verify::{Expect, check_expectation, check_trial, describe_running_image};

const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const CONFIRM_HINT: &str = "the new CPU image is running but not confirmed: run `autd3-firmware-ota \
     confirm` (or simply run the update again) before the next power cycle, otherwise \
     the previous image boots again";

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
    let before = print_versions(&client, "before").await?;
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
    client.set_ack_timeout(Duration::from_secs(5));
    let mut activate_unrebooted = match client.send(Reboot).await {
        Ok(()) => {
            println!("activation acknowledged; the devices reboot in about 100 ms");
            false
        }
        Err(autd3_rs::Error::Timeout { .. }) => {
            eprintln!(
                "warning: not every device acknowledged the activation; the devices that \
                 have not rebooted are activated after the reconnection"
            );
            true
        }
        Err(e) => return Err(e.into()),
    };
    drop(client);

    let wait = update.reboot_wait;
    let expect = release.map_or(Expect::ChangedFrom(&before), Expect::Exactly);
    let attempts = update.reconnect_attempts.max(1);
    loop {
        println!("waiting {wait:?} for the devices to reboot before enumerating them again");
        tokio::time::sleep(wait).await;
        let hint = if activate_unrebooted {
            "the activation was not acknowledged by every device: the devices that received it \
             run the new CPU image unconfirmed, the others still run the previous image and boot \
             the new one at their next power cycle. Once every device is reachable, check which \
             image runs with `verify` and confirm it with `confirm` (or simply run \
             the update again)"
        } else {
            CONFIRM_HINT
        };
        let mut attempt = 0;
        let activated_late = loop {
            attempt += 1;
            match reconnect(
                connection,
                devices,
                !update.no_confirm,
                expect,
                activate_unrebooted,
            )
            .await
            {
                Ok(activated_late) => break activated_late,
                Err(e)
                    if e.downcast_ref::<TrialNotRunning>().is_some()
                        || e.downcast_ref::<NotOnTrial>().is_some() =>
                {
                    return Err(e);
                }
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
        };
        if activated_late.is_empty() {
            break;
        }
        activate_unrebooted = false;
    }
    if update.no_confirm {
        println!("not confirmed (--no-confirm); {CONFIRM_HINT}");
        return Ok(());
    }
    println!("Ok!");
    Ok(())
}

async fn reconnect(
    connection: &Connection,
    devices: usize,
    confirm: bool,
    expect: Expect<'_>,
    activate_unrebooted_devices: bool,
) -> Result<Vec<usize>> {
    let client = connect(connection, devices).await?;
    if activate_unrebooted_devices {
        let activated_late = activate_unrebooted(&client).await?;
        if !activated_late.is_empty() {
            for device in &activated_late {
                println!(
                    "device {device} had not rebooted; activated now, it reboots in about 100 ms"
                );
            }
            return Ok(activated_late);
        }
    }
    check_cpu(&client, confirm, Some(expect)).await?;
    Ok(Vec::new())
}

pub(crate) async fn check_cpu(
    client: &Client,
    confirm_running: bool,
    expect: Option<Expect<'_>>,
) -> Result<()> {
    let label = if confirm_running { "after" } else { "now" };
    let after: Vec<Version> = client
        .read_firmware_version()
        .await?
        .iter()
        .map(|info| info.cpu)
        .collect();
    let images = client.read_running_image().await?;
    for (device, (version, &image)) in after.iter().zip(&images).enumerate() {
        println!(
            "device {device}: CPU firmware {label} = {version} ({})",
            describe_running_image(image)
        );
    }
    if let Some(expect) = expect {
        check_expectation(&after, expect)?;
        check_trial(&images, &after, expect)?;
    }
    if confirm_running {
        client.send(UpdateConfirm).await?;
        println!("running image confirmed on every device");
    }
    Ok(())
}

async fn print_versions(client: &Client, label: &str) -> Result<Vec<Version>> {
    let versions: Vec<Version> = client
        .read_firmware_version()
        .await?
        .iter()
        .map(|info| info.cpu)
        .collect();
    for (device, version) in versions.iter().enumerate() {
        println!("device {device}: CPU firmware {label} = {version}");
    }
    Ok(versions)
}
