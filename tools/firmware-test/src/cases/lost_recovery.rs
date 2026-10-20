use core::time::Duration;
use std::time::Instant;

use anyhow::{Context, Result};

use autd3_rs::commands::{CpuConfig, ForceFan, ReleaseFailsafe, SetCpuConfig, Synchronize, each};
use autd3_rs::{Client, DeviceMask, DeviceState, StateChecker};

use crate::io::{check, prompt};
use crate::{Link, Session, describe, mute, restore_gpio_out};

const RECOVER_INTERVAL: Duration = Duration::from_millis(100);
const DETECTION_WINDOW: Duration = Duration::from_secs(30);
const RECOVERY_WINDOW: Duration = Duration::from_secs(60);
const FAILSAFE_TRIP: Duration = Duration::from_secs(2);
const FAILSAFE_TIMEOUT: Duration = Duration::from_millis(500);
const STICKY_WINDOW: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(10);

fn verdict(ok: bool) -> &'static str {
    if ok { "[OK]" } else { "[FAIL]" }
}

fn snapshot(states: &StateChecker) -> Vec<DeviceState> {
    states
        .check()
        .map(|s| s.devices().to_vec())
        .unwrap_or_default()
}

async fn reaches(
    states: &StateChecker,
    expected: &[DeviceState],
    window: Duration,
) -> Option<Duration> {
    let started = Instant::now();
    while started.elapsed() < window {
        if snapshot(states) == expected {
            return Some(started.elapsed());
        }
        tokio::time::sleep(POLL).await;
    }
    None
}

async fn holds(states: &StateChecker, expected: &[DeviceState], window: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < window {
        if snapshot(states) != expected {
            return false;
        }
        tokio::time::sleep(POLL).await;
    }
    true
}

struct Observed {
    host_trips: Vec<u16>,
    gates: Vec<bool>,
}

async fn observe(client: &Client) -> Result<Observed> {
    let telemetry = client.read_telemetry().await?;
    let fpga = client.read_fpga_state().await?;
    Ok(Observed {
        host_trips: telemetry.iter().map(|t| t.failsafe).collect(),
        gates: fpga.iter().map(|s| s.is_failsafe_active()).collect(),
    })
}

fn cut_off(count: usize, first: usize) -> Vec<DeviceState> {
    (0..count)
        .map(|device| {
            if device < first {
                DeviceState::Ready
            } else {
                DeviceState::Lost
            }
        })
        .collect()
}

async fn open(link: &Link<'_>) -> Result<Session> {
    let client = link.open().await.context("opening the devices")?;
    restore_gpio_out(&client).await;
    if link.mute && !mute(&client).await {
        anyhow::bail!("muting the devices failed");
    }
    Ok(client)
}

async fn recover_until(
    client: &Client,
    expected: DeviceMask,
    window: Duration,
) -> (DeviceMask, Duration) {
    let started = Instant::now();
    let mut recovered = DeviceMask::default();
    while started.elapsed() < window && recovered != expected {
        match client.recover().await {
            Ok(devices) => recovered |= devices,
            Err(e) => println!("  recover() reported: {e}"),
        }
        tokio::time::sleep(RECOVER_INTERVAL).await;
    }
    (recovered, started.elapsed())
}

async fn lose(states: &StateChecker, count: usize, first: usize, how: &str) -> bool {
    let expected = cut_off(count, first);
    check(how);
    let reached = reaches(states, &expected, DETECTION_WINDOW).await;
    if let Some(elapsed) = reached {
        println!(
            "  [OK] device[{first}] and every device behind it turned Lost ({:.1} s after the prompt): {expected:?}",
            elapsed.as_secs_f32()
        );
    } else {
        println!(
            "  [FAIL] expected {expected:?} within {DETECTION_WINDOW:?}, got {:?}",
            snapshot(states)
        );
    }
    reached.is_some()
}

async fn by_cable(link: &Link<'_>, first: usize) -> Result<()> {
    println!("-- cable unplugged and plugged back");
    let client = open(link).await?;
    let failsafe_timeout = FAILSAFE_TIMEOUT.max(link.heartbeat.saturating_mul(4));
    client
        .send(SetCpuConfig::new(CpuConfig {
            failsafe_timeout: Some(failsafe_timeout),
            ..CpuConfig::default()
        }))
        .await
        .context("enabling the host-silence failsafe")?;
    let count = client.num_devices();
    let states = client.state_checker();
    let ready = vec![DeviceState::Ready; count];
    let before = observe(&client).await?;
    println!(
        "  {} every device is Ready with its gate down before the cable is touched: {:?} gates {:?}",
        verdict(snapshot(&states) == ready && before.gates.iter().all(|g| !g)),
        snapshot(&states),
        before.gates
    );

    if !lose(
        &states,
        count,
        first,
        &format!(
            "Unplug the cable on the host side of device[{first}] within {DETECTION_WINDOW:?} and keep it unplugged"
        ),
    )
    .await
    {
        client.close().await?;
        return Ok(());
    }
    tokio::time::sleep(FAILSAFE_TRIP.max(failsafe_timeout.saturating_mul(2))).await;
    check("Plug the cable back and wait until the link LEDs are on");
    println!(
        "  {} the devices stay Lost until recover() is called: {:?}",
        verdict(holds(&states, &cut_off(count, first), STICKY_WINDOW).await),
        snapshot(&states)
    );
    let expected: DeviceMask = (first..count).collect();
    let (recovered, elapsed) = recover_until(&client, expected, RECOVERY_WINDOW).await;
    println!(
        "  {} recover() brought back the devices that were cut off ({:.1} s): {:?}",
        verdict(recovered == expected),
        elapsed.as_secs_f32(),
        recovered.iter().collect::<Vec<_>>()
    );
    let recoveries = client.bus_stats().recoveries();
    println!(
        "  {} the recoveries count the devices that were cut off: {recoveries}",
        verdict(recoveries == (count - first) as u64)
    );

    let sent = client.send(ForceFan { value: false }).await;
    println!(
        "  {} the first send after the recovery completes on every device: {sent:?}",
        verdict(sent.is_ok())
    );
    let after = observe(&client).await?;
    println!(
        "  {} the host-silence failsafe tripped on the devices that were cut off: trips {:?} -> {:?}",
        verdict((first..count).all(|device| after.host_trips[device] > before.host_trips[device])),
        before.host_trips,
        after.host_trips
    );
    println!(
        "  {} the gate stays up on the devices that were cut off and down on the others: gates {:?}",
        verdict(after.gates == (0..count).map(|device| device >= first).collect::<Vec<_>>()),
        after.gates
    );
    check(
        "On the oscilloscope, look at the GPIO[0] outputs: Synchronize has not been sent again, so a long cut may leave them apart",
    );

    match reaches(&states, &ready, RECOVERY_WINDOW).await {
        Some(elapsed) => println!(
            "  [OK] every device is Ready again ({:.1} s)",
            elapsed.as_secs_f32()
        ),
        None => println!(
            "  [FAIL] expected {ready:?} within {RECOVERY_WINDOW:?}, got {:?}",
            snapshot(&states)
        ),
    }
    let synchronized = client
        .send(each(|device| {
            recovered[device.idx()].then_some(Synchronize)
        }))
        .await;
    let released = client.send(ReleaseFailsafe).await;
    let gates = observe(&client).await?.gates;
    println!(
        "  {} Synchronize to the recovered devices and ReleaseFailsafe lower every gate: {synchronized:?} {released:?} gates {gates:?}",
        verdict(synchronized.is_ok() && released.is_ok() && gates.iter().all(|g| !g))
    );
    check("On the oscilloscope, the GPIO[0] output is synchronised across every device again");
    client.close().await?;
    Ok(())
}

async fn by_power(link: &Link<'_>) -> Result<()> {
    println!("-- power-cycled device");
    let client = open(link).await?;
    let count = client.num_devices();
    let states = client.state_checker();
    let last = count - 1;
    if lose(
        &states,
        count,
        last,
        &format!("Power-cycle device[{last}] within {DETECTION_WINDOW:?}"),
    )
    .await
    {
        check("Wait until the device has booted and its link LED is on");
        let recovered = client.recover().await;
        println!(
            "  {} recover() does not bring back a power-cycled device: {recovered:?}",
            verdict(matches!(&recovered, Ok(mask) if mask.is_empty()))
        );
        println!(
            "  {} a power-cycled device stays Lost for {STICKY_WINDOW:?} after it has booted: {:?} recoveries {}",
            verdict(
                holds(&states, &cut_off(count, last), STICKY_WINDOW).await
                    && client.bus_stats().recoveries() == 0
            ),
            snapshot(&states),
            client.bus_stats().recoveries()
        );
    }
    if let Err(e) = client.close().await {
        println!("  closing the client with a Lost device reported: {e}");
    }
    Ok(())
}

async fn confirmed(question: &str) -> bool {
    prompt(&format!("{question} [y/N]"))
        .await
        .trim()
        .eq_ignore_ascii_case("y")
}

pub(crate) async fn run(client: Session, link: &Link<'_>) -> Result<Session> {
    let count = client.num_devices();
    if let Err(e) = client.close().await {
        println!("  closing the client before the case reported: {e}");
    }
    drop(client);

    let answer = prompt(&format!(
        "Index of the first device to cut off (0..{count}, 0 = the cable at the host, empty = the last one)"
    ))
    .await;
    let first = answer
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|device| *device < count)
        .unwrap_or(count - 1);

    if let Err(e) = by_cable(link, first).await {
        println!("  [FAIL] the round stopped: {e:#}");
    }
    if confirmed("Check that a power-cycled device stays Lost?").await
        && let Err(e) = by_power(link).await
    {
        println!("  [FAIL] the round stopped: {e:#}");
    }

    let client = link.reopen().await?;
    describe(&client).await;
    Ok(client)
}
