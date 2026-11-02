use std::time::Duration;

use anyhow::{Context, Result};

use autd3_firmware_writer::image::CpuFirmwareImage;

use crate::cli::Cli;
use crate::session::{Stage, open_and_run};
use crate::verify::{Expect, NotOnTrial, TrialNotRunning, Version};

const RECONNECT_INTERVAL: Duration = Duration::from_secs(2);
const CONFIRM_ONLY_HINT: &str = "the new CPU image is running but not confirmed: rerun the same command with \
     `--confirm-only` (or simply run the update again) before the next power cycle, otherwise \
     the previous image boots again";

pub(crate) async fn main_cpu(cli: &Cli, bytes: &[u8], release: Option<Version>) -> Result<()> {
    let image = CpuFirmwareImage::from_flash_image(bytes)?;
    println!(
        "image: {} bytes, crc32 {:#010x}",
        image.len(),
        image.crc32()
    );
    if cli.verify_only || cli.confirm_only {
        open_and_run(
            cli,
            cli.devices,
            Stage::Verify {
                confirm: cli.confirm_only,
                expect: Expect::Anything,
                activate_unrebooted: false,
                require_trial: false,
            },
        )
        .await?;
        return Ok(());
    }
    let activate = !cli.no_activate;
    let before = open_and_run(
        cli,
        cli.devices,
        Stage::Update {
            image: &image,
            activate,
        },
    )
    .await?;
    if !activate {
        println!(
            "not activated (--no-activate); the committed image boots as a trial at the next \
             reset or power cycle. Run `--confirm-only` once it is up, or it rolls back at the \
             reset after that"
        );
        return Ok(());
    }
    let wait = cli.reboot_wait;
    let expect = release.map_or(Expect::ChangedFrom(&before.cpu_versions), Expect::Exactly);
    let attempts = cli.reconnect_attempts.max(1);
    let mut activate_unrebooted = before.activation_unacknowledged;
    loop {
        println!("waiting {wait:?} for the devices to reboot before enumerating them again");
        tokio::time::sleep(wait).await;
        let stage = Stage::Verify {
            confirm: !cli.no_confirm,
            expect,
            activate_unrebooted,
            require_trial: true,
        };
        let hint = if activate_unrebooted {
            "the activation was not acknowledged by every device: the devices that received it \
             run the new CPU image unconfirmed, the others still run the previous image and boot \
             the new one at their next power cycle. Once every device is reachable, check which \
             image runs with `--verify-only` and confirm it with `--confirm-only` (or simply run \
             the update again)"
        } else {
            CONFIRM_ONLY_HINT
        };
        let mut attempt = 0;
        let outcome = loop {
            attempt += 1;
            match open_and_run(cli, before.devices, stage).await {
                Ok(outcome) => break outcome,
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
        if outcome.activated_late.is_empty() {
            break;
        }
        activate_unrebooted = false;
    }
    if cli.no_confirm {
        println!("not confirmed (--no-confirm); {CONFIRM_ONLY_HINT}");
        return Ok(());
    }
    println!("Ok!");
    Ok(())
}
