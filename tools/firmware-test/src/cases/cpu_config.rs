use core::num::NonZeroU16;
use core::time::Duration;

use anyhow::Result;

use autd3_rs::commands::{
    Clear, ConfigModulation, CpuConfig, PtpConfig, ReleaseFailsafe, SetCpuConfig, SetOutputMask,
    SetSilencer, Synchronize, WriteModulationBuffer,
};
use autd3_rs::value::{LoopBehavior, ModulationBank, SamplingConfig};
use autd3_rs::{DeviceState, Frames};

use crate::{Ctx, failsafe_gates};

pub(crate) async fn silence(ctx: &Ctx<'_>) -> Result<()> {
    ctx.stop_output().await?;
    let masks: Vec<Vec<bool>> = ctx
        .geometry
        .iter()
        .map(|dev| vec![false; dev.num_transducers()])
        .collect();
    ctx.send(SetOutputMask { masks }).await?;
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

pub(crate) fn states(ctx: &Ctx<'_>) -> Vec<DeviceState> {
    crate::device_states(ctx.client)
}

pub(crate) async fn watch(ctx: &Ctx<'_>, label: &str, window: Duration) -> Vec<DeviceState> {
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

async fn failsafe_counts(ctx: &Ctx<'_>) -> Result<Vec<u32>> {
    Ok(ctx
        .client
        .read_telemetry()
        .await?
        .iter()
        .map(|t| u32::from(t.failsafe))
        .collect())
}

async fn set_failsafe_timeout(ctx: &Ctx<'_>, failsafe_timeout: Option<Duration>) -> Result<()> {
    ctx.send(SetCpuConfig::new(CpuConfig {
        failsafe_timeout,
        ..CpuConfig::default()
    }))
    .await?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn run(ctx: &Ctx<'_>) -> Result<()> {
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
        set_failsafe_timeout(ctx, Some(Duration::from_millis(500))).await?;
        tokio::time::sleep(idle).await;
        let counts = failsafe_counts(ctx).await?;
        println!(
            "  {} a 500 ms timeout never trips while the heartbeat ({:?}) runs: {counts:?}",
            verdict(counts.iter().all(|c| *c == 0)),
            ctx.heartbeat
        );

        set_failsafe_timeout(ctx, Some(short)).await?;
        tokio::time::sleep(idle).await;
        let tripped = failsafe_counts(ctx).await?;
        println!(
            "  {} a timeout ({short:?}) shorter than the heartbeat trips on every device: {tripped:?}",
            verdict(tripped.iter().all(|c| *c > 0))
        );
        let gated = failsafe_gates(ctx.client)?;
        println!(
            "  {} the trip raises the failsafe gate on every device",
            verdict(gated.iter().all(|g| *g))
        );

        set_failsafe_timeout(ctx, None).await?;
        let before = failsafe_counts(ctx).await?;
        tokio::time::sleep(idle).await;
        let after = failsafe_counts(ctx).await?;
        println!(
            "  {} a disabled failsafe stops tripping: {before:?} -> {after:?}",
            verdict(before == after)
        );
        let still_gated = failsafe_gates(ctx.client)?;
        ctx.send(ReleaseFailsafe).await?;
        let released = failsafe_gates(ctx.client)?;
        println!(
            "  {} the gate stays up until ReleaseFailsafe lowers it",
            verdict(still_gated.iter().all(|g| *g) && released.iter().all(|g| !g))
        );

        set_failsafe_timeout(ctx, Some(short)).await?;
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
            "  {} Clear disabled the failsafe (the default) and reset the counter: {counts:?}",
            verdict(counts.iter().all(|c| *c == 0))
        );
        let cleared = failsafe_gates(ctx.client)?;
        println!(
            "  {} Clear lowered the failsafe gate",
            verdict(cleared.iter().all(|g| !g))
        );
        Ok(())
    }

    silence(ctx).await?;
    let outcome = async {
        failsafe(ctx).await?;
        ptp(ctx).await
    }
    .await;
    ctx.send(Clear).await?;
    ctx.stop_output().await?;
    outcome
}
