use core::time::Duration;
use std::time::Instant;

use anyhow::Result;

use autd3_rs::commands::ForceFan;
use autd3_rs::{Client, DeviceState, Error};

use crate::io::{check, prompt, wait_enter};
use crate::{Link, Session, describe};

const DETECTION_WINDOW: Duration = Duration::from_secs(30);
const STICKY_WINDOW: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(10);

fn verdict(ok: bool) -> &'static str {
    if ok { "[OK]" } else { "[FAIL]" }
}

fn snapshot(client: &Client) -> Vec<DeviceState> {
    crate::device_states(client)
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn run(client: Session, link: &Link<'_>) -> Result<Session> {
    async fn reaches(
        client: &Client,
        expected: &[DeviceState],
        window: Duration,
    ) -> Option<Duration> {
        let started = Instant::now();
        while started.elapsed() < window {
            if snapshot(client) == expected {
                return Some(started.elapsed());
            }
            tokio::time::sleep(POLL).await;
        }
        None
    }
    async fn holds(client: &Client, expected: &[DeviceState], window: Duration) -> bool {
        let started = Instant::now();
        while started.elapsed() < window {
            if snapshot(client) != expected {
                return false;
            }
            tokio::time::sleep(POLL).await;
        }
        true
    }

    let count = client.num_devices();
    let before = snapshot(&client);
    println!(
        "  {} every device is Ready before the cable is touched: {before:?}",
        verdict(before.iter().all(|d| *d == DeviceState::Ready))
    );

    let answer = prompt(&format!(
        "Index of the first device to cut off (0..{count}, empty = the last one)"
    ))
    .await;
    let first = answer
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|device| *device < count)
        .unwrap_or(count - 1);

    let count = client.num_devices();
    let expected: Vec<DeviceState> = (0..count)
        .map(|device| {
            if device < first {
                DeviceState::Ready
            } else {
                DeviceState::Lost
            }
        })
        .collect();
    check(&format!(
        "Unplug the cable on the host side of device[{first}], or power-cycle device[{first}], within {DETECTION_WINDOW:?}"
    ));
    match reaches(&client, &expected, DETECTION_WINDOW).await {
        Some(elapsed) => println!(
            "  [OK] device[{first}] and every device behind it turned Lost, the others stayed Ready ({:.1} s after the prompt): {expected:?}",
            elapsed.as_secs_f32()
        ),
        None => println!(
            "  [FAIL] expected {expected:?} within {DETECTION_WINDOW:?}, got {:?}",
            snapshot(&client)
        ),
    }
    let read = client.read_fpga_state().await.map(|values| values.len());
    let write = client.send(ForceFan { value: false }).await;
    if first == 0 {
        println!(
            "  {} a read times out when every device is Lost: {read:?}",
            verdict(matches!(read, Err(Error::Timeout { .. })))
        );
        println!(
            "  {} a write times out when every device is Lost: {write:?}",
            verdict(matches!(write, Err(Error::Timeout { .. })))
        );
    } else {
        println!(
            "  {} a read names the first Lost device instead of returning partial values: {read:?}",
            verdict(matches!(read, Err(Error::DeviceLost { device }) if device == first))
        );
        println!(
            "  {} a write still completes on the devices that are left: {write:?}",
            verdict(write.is_ok())
        );
    }
    wait_enter(
        "Plug the cable back (or wait until the device has booted) and wait for the link LED",
    )
    .await;
    println!(
        "  {} a Lost device stays Lost for {STICKY_WINDOW:?} after the link is back: {:?}",
        verdict(holds(&client, &expected, STICKY_WINDOW).await),
        snapshot(&client)
    );

    if let Err(e) = client.close() {
        println!("  closing the client with Lost devices reported: {e}");
    }
    drop(client);

    let started = Instant::now();
    let client = link.reopen().await?;
    let reopened = snapshot(&client);
    println!(
        "  {} reopening brings every device back to Ready ({:.1} s): {reopened:?}",
        verdict(reopened.len() == count && reopened.iter().all(|d| *d == DeviceState::Ready)),
        started.elapsed().as_secs_f32()
    );
    describe(&client).await;
    Ok(client)
}
