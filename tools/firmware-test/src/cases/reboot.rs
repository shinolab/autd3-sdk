use core::time::Duration;
use std::time::Instant;

use anyhow::Result;

use autd3_rs::commands::Distribution;
use autd3_rs::protocol::Cmd;
use autd3_rs::{Client, Datagram, DeviceState, Frame};

use crate::{Link, Session, describe};

const GONE_WINDOW: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);

fn verdict(ok: bool) -> &'static str {
    if ok { "[OK]" } else { "[FAIL]" }
}

async fn request_reboot(client: &Client) -> Result<()> {
    let datagrams = [Datagram::no_payload(Cmd::Reboot)];
    client
        .send_frame(Frame::new(Distribution::Broadcast, &datagrams))
        .await?
        .await?
        .check()?;
    Ok(())
}

async fn gone(client: &Client, window: Duration) -> Result<(bool, Vec<DeviceState>)> {
    let states = client.state_checker();
    let started = Instant::now();
    loop {
        client.send_heartbeat()?;
        tokio::time::sleep(POLL).await;
        let seen = states.check()?.devices().to_vec();
        let all_lost = seen.iter().all(|d| *d == DeviceState::Lost);
        if all_lost || started.elapsed() >= window {
            return Ok((all_lost, seen));
        }
    }
}

async fn reboot_all(client: &Client, window: Duration) -> Result<(bool, Vec<DeviceState>)> {
    request_reboot(client).await?;
    gone(client, window).await
}

pub(crate) async fn run(client: Session, link: &Link<'_>) -> Result<Session> {
    let devices = client.num_devices();
    let before = client.read_firmware_version().await;
    if let Err(e) = &before {
        println!("  reading the firmware version before the reboot failed: {e}");
    }

    let window = GONE_WINDOW.max(link.option.lost_timeout.saturating_mul(2));
    let started = Instant::now();
    let outcome = reboot_all(&client, window).await;
    drop(client);
    match outcome {
        Ok((all_lost, seen)) => {
            println!("  [OK] every device acknowledged Reboot");
            println!(
                "  {} every device stopped answering under its unit ID within {window:?} of the acknowledgement: {seen:?}",
                verdict(all_lost)
            );
        }
        Err(e) => println!("  [FAIL] Reboot was not delivered to every device: {e:#}"),
    }

    let client = link.reopen().await?;
    let states = client
        .state_checker()
        .check()
        .map(|s| s.devices().to_vec())
        .unwrap_or_default();
    println!(
        "  {} the devices enumerate and synchronise again {:.1} s after the Reboot request: {states:?}",
        verdict(states.len() == devices && states.iter().all(|d| *d == DeviceState::Ready)),
        started.elapsed().as_secs_f32()
    );

    match (before, client.read_firmware_version().await) {
        (Ok(before), Ok(after)) => println!(
            "  {} every device runs the same firmware as before the reboot",
            verdict(before.len() == after.len() && before.iter().zip(&after).all(|(b, a)| b == a))
        ),
        (_, Err(e)) => println!("  [FAIL] reading the firmware version after the reboot: {e}"),
        (Err(_), Ok(_)) => {}
    }
    match client.read_telemetry().await {
        Ok(telemetry) => {
            let failures: Vec<u8> = telemetry.iter().map(|t| t.boot_failure).collect();
            println!(
                "  {} no device reports a boot failure: {failures:?}",
                verdict(failures.iter().all(|c| *c == 0))
            );
        }
        Err(e) => println!("  [FAIL] reading the telemetry after the reboot: {e}"),
    }
    describe(&client).await;
    Ok(client)
}
