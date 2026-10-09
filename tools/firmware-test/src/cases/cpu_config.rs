use core::num::{NonZeroU16, NonZeroU32};
use core::time::Duration;

use anyhow::Result;

use autd3_rs::commands::{
    ActivateModulationBank, Clear, ConfigModulation, CpuConfig, PtpConfig, ReleaseFailsafe,
    SetCpuConfig, SetOutputMask, SetSilencer, Synchronize, WriteModulationBuffer,
};
use autd3_rs::value::{LoopBehavior, ModulationBank, SamplingConfig, TransitionMode};
use autd3_rs::{DeviceState, Error, Frames, Telemetry};

use crate::Ctx;
use crate::cases::pattern_util::{activate_mod_bank, expect_firmware_error, expect_firmware_ok};
use crate::cases::{ERR_FPGA_TIMEOUT, ERR_INVALID_PAYLOAD, ERR_MISS_TRANSITION_TIME};

pub(crate) async fn silence(ctx: &Ctx<'_>) -> Result<()> {
    ctx.client.silent_stop().await?;
    let masks: Vec<Vec<bool>> = ctx
        .geometry
        .iter()
        .map(|dev| vec![false; dev.num_transducers()])
        .collect();
    ctx.send(SetOutputMask { masks: &masks }).await?;
    ctx.send(SetSilencer::disable()).await?;
    for (bank, loop_behavior) in [
        (ModulationBank::B0, LoopBehavior::Infinite),
        (ModulationBank::B1, LoopBehavior::ONCE),
    ] {
        ctx.send(WriteModulationBuffer {
            bank,
            offset: 0,
            data: &[0, 0],
        })
        .await?;
        ctx.send(ConfigModulation {
            bank,
            config: SamplingConfig::new(NonZeroU16::MAX),
            size: 2,
            loop_behavior,
        })
        .await?;
    }
    Ok(())
}

fn at(ctx: &Ctx<'_>, ahead: Duration) -> Result<Frames, Error> {
    let time = ctx.client.device_time_now()? + ahead;
    Frames::encode(
        ctx.client.geometry(),
        ActivateModulationBank {
            bank: ModulationBank::B1,
            transition_mode: TransitionMode::SysTime { time },
        },
    )
}

async fn margin(ctx: &Ctx<'_>) -> Result<()> {
    println!("-- SysTime transition margin");
    expect_firmware_ok(
        ctx,
        "default margin, 1 s ahead",
        at(ctx, Duration::from_secs(1)),
    )
    .await;
    activate_mod_bank(ctx, ModulationBank::B0, TransitionMode::Immediate).await?;
    expect_firmware_error(
        ctx,
        "default margin (10 ms), 3 ms ahead",
        at(ctx, Duration::from_millis(3)),
        ERR_MISS_TRANSITION_TIME,
    )
    .await;

    ctx.send(SetCpuConfig::new(CpuConfig {
        sys_time_transition_margin: Duration::ZERO,
        ..CpuConfig::default()
    }))
    .await?;
    expect_firmware_ok(
        ctx,
        "zero margin, 3 ms ahead",
        at(ctx, Duration::from_millis(3)),
    )
    .await;
    activate_mod_bank(ctx, ModulationBank::B0, TransitionMode::Immediate).await?;

    ctx.send(SetCpuConfig::new(CpuConfig {
        sys_time_transition_margin: Duration::from_secs(2),
        ..CpuConfig::default()
    }))
    .await?;
    expect_firmware_error(
        ctx,
        "2 s margin, 1 s ahead",
        at(ctx, Duration::from_secs(1)),
        ERR_MISS_TRANSITION_TIME,
    )
    .await;
    expect_firmware_ok(
        ctx,
        "2 s margin, 3 s ahead",
        at(ctx, Duration::from_secs(3)),
    )
    .await;
    activate_mod_bank(ctx, ModulationBank::B0, TransitionMode::Immediate).await?;

    expect_firmware_error(
        ctx,
        "zero update_activate_delay",
        Frames::encode(
            ctx.client.geometry(),
            SetCpuConfig::new(CpuConfig {
                update_activate_delay: Duration::ZERO,
                ..CpuConfig::default()
            }),
        ),
        ERR_INVALID_PAYLOAD,
    )
    .await;
    expect_firmware_error(
        ctx,
        "the rejected config left the 2 s margin in place, 1 s ahead",
        at(ctx, Duration::from_secs(1)),
        ERR_MISS_TRANSITION_TIME,
    )
    .await;

    ctx.send(Clear).await?;
    silence(ctx).await?;
    expect_firmware_ok(
        ctx,
        "Clear restored the default margin, 1 s ahead",
        at(ctx, Duration::from_secs(1)),
    )
    .await;
    activate_mod_bank(ctx, ModulationBank::B0, TransitionMode::Immediate).await
}

async fn latch_polls(ctx: &Ctx<'_>) -> Result<()> {
    println!("-- FPGA latch polls");
    ctx.send(SetCpuConfig::new(CpuConfig {
        fpga_wait_update_max_polls: NonZeroU32::MIN,
        ..CpuConfig::default()
    }))
    .await?;
    expect_firmware_error(
        ctx,
        "a single poll cannot see the latch complete",
        Frames::encode(ctx.client.geometry(), SetSilencer::disable()),
        ERR_FPGA_TIMEOUT,
    )
    .await;
    ctx.send(Clear).await?;
    silence(ctx).await?;
    expect_firmware_ok(
        ctx,
        "Clear restored the default polls",
        Frames::encode(ctx.client.geometry(), SetSilencer::disable()),
    )
    .await;
    Ok(())
}

fn states(ctx: &Ctx<'_>) -> Vec<DeviceState> {
    ctx.states
        .check()
        .map(|s| s.devices().to_vec())
        .unwrap_or_default()
}

async fn watch(ctx: &Ctx<'_>, label: &str, window: Duration) -> Vec<DeviceState> {
    let deadline = std::time::Instant::now() + window;
    let mut seen = states(ctx);
    let mut changes = 0u32;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let now = states(ctx);
        if now != seen {
            changes += 1;
            seen = now;
        }
    }
    println!("  {label}: {seen:?} ({changes} change(s) in {window:?})");
    seen
}

async fn ptp(ctx: &Ctx<'_>) -> Result<()> {
    println!("-- PTP");
    let all_ready = |s: &[DeviceState]| s.iter().all(|d| *d == DeviceState::Ready);
    let verdict = |ok: bool| if ok { "[OK]" } else { "[FAIL]" };

    let s = watch(ctx, "default", Duration::from_secs(1)).await;
    println!(
        "  {} every device is Ready before any change",
        verdict(all_ready(&s))
    );

    ctx.send(SetCpuConfig::new(CpuConfig {
        ptp: PtpConfig {
            sync_interval: Duration::from_millis(8),
            holdover: Duration::from_millis(500),
            lock_samples: NonZeroU16::new(32).unwrap(),
            ..PtpConfig::default()
        },
        ..CpuConfig::default()
    }))
    .await?;
    let s = watch(
        ctx,
        "8 ms interval / 500 ms holdover",
        Duration::from_secs(3),
    )
    .await;
    println!(
        "  {} the lock survives a benign change",
        verdict(all_ready(&s))
    );

    ctx.send(SetCpuConfig::new(CpuConfig {
        ptp: PtpConfig {
            holdover: Duration::from_millis(1),
            ..PtpConfig::default()
        },
        ..CpuConfig::default()
    }))
    .await?;
    let s = watch(ctx, "1 ms holdover", Duration::from_secs(2)).await;
    println!(
        "  {} a holdover shorter than the sync interval drops the lock of the slaves",
        verdict(s.len() > 1 && s[1..].iter().all(|d| *d == DeviceState::Syncing))
    );
    println!(
        "  {} the grandmaster is unaffected",
        verdict(s.first() == Some(&DeviceState::Ready))
    );

    ctx.send(SetCpuConfig::new(CpuConfig::default())).await?;
    let s = watch(ctx, "default again", Duration::from_secs(8)).await;
    println!("  {} every device locks again", verdict(all_ready(&s)));
    ctx.send(Synchronize).await?;
    Ok(())
}

async fn failsafe_counts(ctx: &Ctx<'_>) -> Result<Vec<u32>> {
    Ok(ctx
        .client
        .read_telemetry()
        .await?
        .iter()
        .map(|t| t.get(Telemetry::Failsafe))
        .collect())
}

async fn failsafe(ctx: &Ctx<'_>) -> Result<()> {
    println!("-- Failsafe timeout");
    let verdict = |ok: bool| if ok { "[OK]" } else { "[FAIL]" };
    let idle = Duration::from_secs(1);
    if ctx.heartbeat < Duration::from_millis(2) || ctx.heartbeat > Duration::from_millis(200) {
        println!(
            "  [SKIP] needs a heartbeat of 2..=200 ms to trip the failsafe between heartbeats (got {:?})",
            ctx.heartbeat
        );
        return Ok(());
    }
    let short = Duration::from_millis(u64::from(ctx.heartbeat.subsec_millis() / 2));

    ctx.send(Clear).await?;
    silence(ctx).await?;
    tokio::time::sleep(idle).await;
    let counts = failsafe_counts(ctx).await?;
    println!(
        "  {} the default timeout (500 ms) never trips while the heartbeat ({:?}) runs: {counts:?}",
        verdict(counts.iter().all(|c| *c == 0)),
        ctx.heartbeat
    );

    ctx.send(SetCpuConfig::new(CpuConfig {
        failsafe_timeout: Some(short),
        ..CpuConfig::default()
    }))
    .await?;
    tokio::time::sleep(idle).await;
    let tripped = failsafe_counts(ctx).await?;
    println!(
        "  {} a timeout ({short:?}) shorter than the heartbeat trips on every device: {tripped:?}",
        verdict(tripped.iter().all(|c| *c > 0))
    );
    let gated = ctx.client.read_fpga_state().await?;
    println!(
        "  {} the trip raises the failsafe gate on every device",
        verdict(gated.iter().all(|s| s.is_failsafe_active()))
    );

    ctx.send(SetCpuConfig::new(CpuConfig {
        failsafe_timeout: None,
        ..CpuConfig::default()
    }))
    .await?;
    let before = failsafe_counts(ctx).await?;
    tokio::time::sleep(idle).await;
    let after = failsafe_counts(ctx).await?;
    println!(
        "  {} a disabled failsafe stops tripping: {before:?} -> {after:?}",
        verdict(before == after)
    );
    let still_gated = ctx.client.read_fpga_state().await?;
    ctx.send(ReleaseFailsafe).await?;
    let released = ctx.client.read_fpga_state().await?;
    println!(
        "  {} the gate stays up until ReleaseFailsafe lowers it",
        verdict(
            still_gated.iter().all(|s| s.is_failsafe_active())
                && released.iter().all(|s| !s.is_failsafe_active())
        )
    );

    ctx.send(SetCpuConfig::new(CpuConfig {
        failsafe_timeout: Some(short),
        ..CpuConfig::default()
    }))
    .await?;
    tokio::time::sleep(idle).await;
    let rearmed = failsafe_counts(ctx).await?;
    println!(
        "  {} enabling it again trips again: {after:?} -> {rearmed:?}",
        verdict(rearmed.iter().zip(&after).all(|(now, was)| now > was))
    );

    let zero = Frames::encode(
        ctx.client.geometry(),
        SetCpuConfig::new(CpuConfig {
            failsafe_timeout: Some(Duration::ZERO),
            ..CpuConfig::default()
        }),
    );
    println!(
        "  {} a zero timeout is refused when encoded instead of disabling the failsafe",
        verdict(zero.is_err())
    );

    ctx.send(Clear).await?;
    silence(ctx).await?;
    tokio::time::sleep(idle).await;
    let counts = failsafe_counts(ctx).await?;
    println!(
        "  {} Clear restored the default timeout and the counter: {counts:?}",
        verdict(counts.iter().all(|c| *c == 0))
    );
    let cleared = ctx.client.read_fpga_state().await?;
    println!(
        "  {} Clear lowered the failsafe gate",
        verdict(cleared.iter().all(|s| !s.is_failsafe_active()))
    );
    Ok(())
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    silence(ctx).await?;
    let outcome = async {
        margin(ctx).await?;
        latch_polls(ctx).await?;
        failsafe(ctx).await?;
        ptp(ctx).await
    }
    .await;
    ctx.send(Clear).await?;
    ctx.client.silent_stop().await?;
    outcome
}
