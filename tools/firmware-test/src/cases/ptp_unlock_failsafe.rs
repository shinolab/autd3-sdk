use core::time::Duration;
use std::time::Instant;

use anyhow::Result;

use autd3_rs::commands::{Clear, CpuConfig, PtpConfig, ReleaseFailsafe, SetCpuConfig, Synchronize};
use autd3_rs::{DeviceState, Frames};

use crate::Ctx;
use crate::cases::ERR_FAILSAFE_CONDITION_ACTIVE;
use crate::cases::cpu_config::{silence, states, watch};
use crate::cases::pattern_util::{expect_firmware_error, expect_firmware_ok};
use crate::io::wait_enter;

const TIMEOUT: Duration = Duration::from_secs(1);
const SETTLE: Duration = Duration::from_secs(3);
const RELOCK: Duration = Duration::from_secs(8);
const CABLE_LOST_TIMEOUT: Duration = Duration::from_secs(20);
const CABLE_RECOVERY: Duration = Duration::from_secs(30);

fn verdict(ok: bool) -> &'static str {
    if ok { "[OK]" } else { "[FAIL]" }
}

fn unlocked_ptp() -> PtpConfig {
    PtpConfig {
        holdover: Duration::from_millis(1),
        lock_threshold: Duration::ZERO,
        ..PtpConfig::default()
    }
}

struct Observed {
    ptp_trips: Vec<u16>,
    host_trips: Vec<u16>,
    gates: Vec<bool>,
}

async fn observe(ctx: &Ctx<'_>) -> Result<Observed> {
    let telemetry = ctx.client.read_telemetry().await?;
    let fpga = ctx.client.read_fpga_state().await?;
    Ok(Observed {
        ptp_trips: telemetry.iter().map(|t| t.ptp_unlock_failsafe).collect(),
        host_trips: telemetry.iter().map(|t| t.failsafe).collect(),
        gates: fpga.iter().map(|s| s.is_failsafe_active()).collect(),
    })
}

fn untouched(seen: &Observed) -> bool {
    seen.ptp_trips.iter().all(|c| *c == 0) && seen.gates.iter().all(|g| !g)
}

async fn config(ctx: &Ctx<'_>, config: CpuConfig) -> Result<()> {
    ctx.send(SetCpuConfig::new(config)).await
}

async fn by_holdover(ctx: &Ctx<'_>) -> Result<()> {
    println!("-- lock dropped by a holdover shorter than the sync interval");
    let seen = observe(ctx).await?;
    println!(
        "  {} nothing has tripped before any change: trips {:?} gates {:?}",
        verdict(untouched(&seen)),
        seen.ptp_trips,
        seen.gates
    );

    config(
        ctx,
        CpuConfig {
            ptp_unlock_failsafe_timeout: Some(TIMEOUT),
            ..CpuConfig::default()
        },
    )
    .await?;
    let s = watch(ctx, "timeout armed, default PTP", SETTLE).await;
    let seen = observe(ctx).await?;
    println!(
        "  {} an armed timeout ({TIMEOUT:?}) never trips while every device is locked: trips {:?}",
        verdict(s.iter().all(|d| *d == DeviceState::Ready) && untouched(&seen)),
        seen.ptp_trips
    );

    config(
        ctx,
        CpuConfig {
            ptp: unlocked_ptp(),
            ..CpuConfig::default()
        },
    )
    .await?;
    let s = watch(ctx, "timeout disabled, 1 ms holdover", SETTLE).await;
    let slaves_unlocked = s.len() > 1 && s[1..].iter().all(|d| *d == DeviceState::Syncing);
    println!(
        "  {} the slaves lose the lock and the grandmaster keeps it",
        verdict(slaves_unlocked && s.first() == Some(&DeviceState::Ready))
    );
    let seen = observe(ctx).await?;
    println!(
        "  {} a disabled timeout (None) never trips while the slaves are unlocked: trips {:?} gates {:?}",
        verdict(untouched(&seen)),
        seen.ptp_trips,
        seen.gates
    );

    config(
        ctx,
        CpuConfig {
            ptp: unlocked_ptp(),
            ptp_unlock_failsafe_timeout: Some(TIMEOUT),
            ..CpuConfig::default()
        },
    )
    .await?;
    tokio::time::sleep(SETTLE).await;
    let seen = observe(ctx).await?;
    println!(
        "  {} every slave trips once the lock has been gone for {TIMEOUT:?}: trips {:?}",
        verdict(seen.ptp_trips.len() > 1 && seen.ptp_trips[1..].iter().all(|c| *c > 0)),
        seen.ptp_trips
    );
    println!(
        "  {} the trip raises the failsafe gate on every slave: gates {:?}",
        verdict(seen.gates.len() > 1 && seen.gates[1..].iter().all(|g| *g)),
        seen.gates
    );
    println!(
        "  {} the grandmaster neither trips nor raises its gate",
        verdict(seen.ptp_trips.first() == Some(&0) && seen.gates.first() == Some(&false))
    );
    println!(
        "  {} the host-silence failsafe did not trip, so the gate is the PTP one: host trips {:?}",
        verdict(seen.host_trips.iter().all(|c| *c == 0)),
        seen.host_trips
    );
    release_by_relocking(ctx).await
}

async fn release_by_relocking(ctx: &Ctx<'_>) -> Result<()> {
    expect_firmware_error(
        ctx,
        "ReleaseFailsafe while the slaves are still unlocked",
        Frames::encode(ctx.client.geometry(), ReleaseFailsafe),
        ERR_FAILSAFE_CONDITION_ACTIVE,
    )
    .await;
    let seen = observe(ctx).await?;
    println!(
        "  {} the refused release left the gate up on every slave: gates {:?}",
        verdict(seen.gates.len() > 1 && seen.gates[1..].iter().all(|g| *g)),
        seen.gates
    );

    config(
        ctx,
        CpuConfig {
            ptp_unlock_failsafe_timeout: Some(TIMEOUT),
            ..CpuConfig::default()
        },
    )
    .await?;
    let s = watch(ctx, "default PTP, timeout still armed", RELOCK).await;
    println!(
        "  {} every device locks again",
        verdict(s.iter().all(|d| *d == DeviceState::Ready))
    );
    let seen = observe(ctx).await?;
    println!(
        "  {} locking again does not lower the gate by itself: gates {:?}",
        verdict(seen.gates.len() > 1 && seen.gates[1..].iter().all(|g| *g)),
        seen.gates
    );
    expect_firmware_ok(
        ctx,
        "ReleaseFailsafe after the lock is back",
        Frames::encode(ctx.client.geometry(), ReleaseFailsafe),
    )
    .await;
    let seen = observe(ctx).await?;
    println!(
        "  {} the release lowered every gate: gates {:?}",
        verdict(seen.gates.iter().all(|g| !g)),
        seen.gates
    );
    ctx.send(Synchronize).await?;
    Ok(())
}

async fn all_ready_within(ctx: &Ctx<'_>, window: Duration) -> (bool, Duration) {
    let started = Instant::now();
    loop {
        if states(ctx).iter().all(|d| *d == DeviceState::Ready) {
            return (true, started.elapsed());
        }
        if started.elapsed() >= window {
            return (false, started.elapsed());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn by_cable(ctx: &Ctx<'_>) -> Result<()> {
    println!("-- lock dropped by unplugging a cable");
    if ctx.lost_timeout < CABLE_LOST_TIMEOUT {
        println!(
            "  [SKIP] the unplugged device would turn Lost after {:?} and Lost needs a reopen, which clears the counters; rerun with --lost-timeout-ms {} or more",
            ctx.lost_timeout,
            CABLE_LOST_TIMEOUT.as_millis()
        );
        return Ok(());
    }
    let last = ctx.client.num_devices() - 1;

    ctx.send(Clear).await?;
    silence(ctx).await?;
    config(
        ctx,
        CpuConfig {
            failsafe_timeout: None,
            ptp_unlock_failsafe_timeout: Some(TIMEOUT),
            ..CpuConfig::default()
        },
    )
    .await?;
    wait_enter(&format!(
        "Unplug the cable on the host side of device[{last}] for about 5 seconds, plug it back, and wait for the link LED"
    ))
    .await;
    let (ready, elapsed) = all_ready_within(ctx, CABLE_RECOVERY).await;
    println!(
        "  {} every device answers and is locked again after {:.1} s: {:?}",
        verdict(ready),
        elapsed.as_secs_f32(),
        states(ctx)
    );

    let seen = observe(ctx).await?;
    println!(
        "  {} device[{last}] tripped while it was cut off: trips {:?}",
        verdict(seen.ptp_trips[last] > 0),
        seen.ptp_trips
    );
    println!(
        "  {} the trip raised the failsafe gate on device[{last}]: gates {:?}",
        verdict(seen.gates[last]),
        seen.gates
    );
    println!(
        "  {} the devices that stayed connected neither tripped nor raised their gate",
        verdict(
            seen.ptp_trips[..last].iter().all(|c| *c == 0) && seen.gates[..last].iter().all(|g| !g)
        )
    );
    println!(
        "  {} the disabled host-silence failsafe did not trip: host trips {:?}",
        verdict(seen.host_trips.iter().all(|c| *c == 0)),
        seen.host_trips
    );

    config(ctx, CpuConfig::default()).await?;
    expect_firmware_ok(
        ctx,
        "ReleaseFailsafe after the cable is back",
        Frames::encode(ctx.client.geometry(), ReleaseFailsafe),
    )
    .await;
    let seen = observe(ctx).await?;
    println!(
        "  {} the release lowered every gate: gates {:?}",
        verdict(seen.gates.iter().all(|g| !g)),
        seen.gates
    );
    ctx.send(Synchronize).await?;
    Ok(())
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    if ctx.client.num_devices() < 2 {
        println!("  [SKIP] needs at least 2 devices: only a slave can lose the PTP lock");
        return Ok(());
    }
    ctx.send(Clear).await?;
    silence(ctx).await?;
    let outcome = async {
        by_holdover(ctx).await?;
        by_cable(ctx).await
    }
    .await;
    ctx.send(Clear).await?;
    ctx.client.silent_stop().await?;
    outcome
}
