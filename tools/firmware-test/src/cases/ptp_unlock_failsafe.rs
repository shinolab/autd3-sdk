use core::time::Duration;
use std::time::Instant;

use anyhow::Result;

use autd3_rs::commands::{
    Clear, CpuConfig, GpioOut, PtpConfig, ReleaseFailsafe, SetCpuConfig, SetGpioOut, SetOutputMask,
    Synchronize,
};
use autd3_rs::units::Hz;
use autd3_rs::value::SamplingConfig;
use autd3_rs::{DeviceState, Frames};
use autd3_rs_modulation::{SineOption, modulation_buffer, sine};

use crate::Ctx;
use crate::cases::ERR_FAILSAFE_CONDITION_ACTIVE;
use crate::cases::cpu_config::{states, watch};
use crate::cases::pattern_util::{
    expect_firmware_error, expect_firmware_ok, focus_at, send_pattern_mod,
};
use crate::io::wait_enter;

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

async fn emit(ctx: &Ctx<'_>) -> Result<()> {
    ctx.send(Clear).await?;
    ctx.send(SetGpioOut {
        outputs: [
            GpioOut::BaseSignal,
            GpioOut::Off,
            GpioOut::Off,
            GpioOut::Off,
        ],
    })
    .await?;
    let masks: Vec<Vec<bool>> = ctx
        .geometry
        .iter()
        .map(|dev| vec![true; dev.num_transducers()])
        .collect();
    ctx.send(SetOutputMask { masks }).await?;
    let focus = focus_at(ctx.geometry, [0.0, 0.0, 150.0], 0xFF);
    let mut modbuf = modulation_buffer();
    sine(
        150 * Hz,
        &SineOption::default(),
        SamplingConfig::FREQ_4K,
        &mut modbuf,
    )?;
    send_pattern_mod(ctx, &focus, &modbuf, SamplingConfig::FREQ_4K).await?;
    wait_enter(
        "Every device emits a focus 150 mm above its centre modulated at 150 Hz and outputs the base signal on GPIO[0]",
    )
    .await;
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn run(ctx: &Ctx<'_>) -> Result<()> {
    async fn all_ready_within(ctx: &Ctx<'_>, window: Duration) -> Result<(bool, Duration)> {
        let started = Instant::now();
        loop {
            if states(ctx)
                .await
                .is_ok_and(|s| s.iter().all(|d| *d == DeviceState::Ready))
            {
                return Ok((true, started.elapsed()));
            }
            if started.elapsed() >= window {
                return Ok((false, started.elapsed()));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    if ctx.client.num_devices() < 2 {
        println!("  [SKIP] needs at least 2 devices: only a slave can lose the PTP lock");
        return Ok(());
    }
    emit(ctx).await?;
    let outcome = async {
        println!("-- lock dropped by a holdover shorter than the sync interval");
        let gates = ctx.failsafe_gates().await?;
        println!(
            "  {} nothing has tripped before any change: gates {gates:?}",
            verdict(gates.iter().all(|g| !g))
        );

        ctx.send(SetCpuConfig::new(CpuConfig {
            ptp_unlock_failsafe_timeout: Some(Duration::from_secs(1)),
            ..CpuConfig::default()
        }))
        .await?;
        let s = watch(ctx, "timeout armed, default PTP", Duration::from_secs(3)).await?;
        let gates = ctx.failsafe_gates().await?;
        println!(
            "  {} an armed timeout (1s) never trips while every device is locked: gates {gates:?}",
            verdict(s.iter().all(|d| *d == DeviceState::Ready) && gates.iter().all(|g| !g))
        );

        ctx.send(SetCpuConfig::new(CpuConfig {
            ptp: unlocked_ptp(),
            ..CpuConfig::default()
        }))
        .await?;
        let s = watch(
            ctx,
            "timeout disabled, 1 ms holdover",
            Duration::from_secs(3),
        )
        .await?;
        let slaves_unlocked = s.len() > 1 && s[1..].iter().all(|d| *d == DeviceState::Syncing);
        println!(
            "  {} the slaves lose the lock and the grandmaster keeps it",
            verdict(slaves_unlocked && s.first() == Some(&DeviceState::Ready))
        );
        let gates = ctx.failsafe_gates().await?;
        println!(
            "  {} a disabled timeout (None) never trips while the slaves are unlocked: gates {gates:?}",
            verdict(gates.iter().all(|g| !g))
        );

        ctx.send(SetCpuConfig::new(CpuConfig {
            ptp: unlocked_ptp(),
            ptp_unlock_failsafe_timeout: Some(Duration::from_secs(1)),
            ..CpuConfig::default()
        }))
        .await?;
        tokio::time::sleep(Duration::from_secs(3)).await;
        let gates = ctx.failsafe_gates().await?;
        println!(
            "  {} every slave raises its failsafe gate once the lock has been gone for 1s: gates {gates:?}",
            verdict(gates.len() > 1 && gates[1..].iter().all(|g| *g))
        );
        println!(
            "  {} the grandmaster keeps its gate down",
            verdict(gates.first() == Some(&false))
        );
        wait_enter("Every slave stopped emitting, and the grandmaster still emits").await;
        expect_firmware_error(
            ctx,
            "ReleaseFailsafe while the slaves are still unlocked",
            Frames::encode(ctx.client.geometry(), ReleaseFailsafe),
            ERR_FAILSAFE_CONDITION_ACTIVE,
        )
        .await;
        let gates = ctx.failsafe_gates().await?;
        println!(
            "  {} the refused release left the gate up on every slave: gates {gates:?}",
            verdict(gates.len() > 1 && gates[1..].iter().all(|g| *g))
        );
        ctx.send(SetCpuConfig::new(CpuConfig {
            ptp_unlock_failsafe_timeout: Some(Duration::from_secs(1)),
            ..CpuConfig::default()
        }))
        .await?;
        let s = watch(
            ctx,
            "default PTP, timeout still armed",
            Duration::from_secs(8),
        )
        .await?;
        println!(
            "  {} every device locks again",
            verdict(s.iter().all(|d| *d == DeviceState::Ready))
        );
        let gates = ctx.failsafe_gates().await?;
        println!(
            "  {} locking again does not lower the gate by itself: gates {gates:?}",
            verdict(gates.len() > 1 && gates[1..].iter().all(|g| *g))
        );
        wait_enter("Every slave is still silent after locking again").await;
        expect_firmware_ok(
            ctx,
            "ReleaseFailsafe after the lock is back",
            Frames::encode(ctx.client.geometry(), ReleaseFailsafe),
        )
        .await;
        let gates = ctx.failsafe_gates().await?;
        println!(
            "  {} the release lowered every gate: gates {gates:?}",
            verdict(gates.iter().all(|g| !g))
        );
        wait_enter("Every device emits again").await;
        ctx.send(Synchronize).await?;

        println!("-- lock dropped by unplugging a cable");
        async {
            let last = ctx.client.num_devices() - 1;

            emit(ctx).await?;
            ctx.send(SetCpuConfig::new(CpuConfig {
                ptp_unlock_failsafe_timeout: Some(Duration::from_secs(1)),
                ..CpuConfig::default()
            }))
            .await?;
            wait_enter(&format!(
                "Unplug the cable on the host side of device[{last}] for about 5 seconds, plug it back, and wait for the link LED"
            ))
            .await;
            let (ready, elapsed) = all_ready_within(ctx, Duration::from_secs(30)).await?;
            println!(
                "  {} every device answers and is locked again after {:.1} s: {:?}",
                verdict(ready),
                elapsed.as_secs_f32(),
                states(ctx).await?
            );

            let gates = ctx.failsafe_gates().await?;
            println!(
                "  {} device[{last}] raised its failsafe gate while it was cut off: gates {gates:?}",
                verdict(gates[last])
            );
            println!(
                "  {} the devices that stayed connected kept their gate down",
                verdict(gates[..last].iter().all(|g| !g))
            );
            wait_enter(&format!(
                "device[{last}] is silent, and every other device still emits"
            ))
            .await;

            ctx.send(SetCpuConfig::new(CpuConfig::default())).await?;
            expect_firmware_ok(
                ctx,
                "ReleaseFailsafe after the cable is back",
                Frames::encode(ctx.client.geometry(), ReleaseFailsafe),
            )
            .await;
            let gates = ctx.failsafe_gates().await?;
            println!(
                "  {} the release lowered every gate: gates {gates:?}",
                verdict(gates.iter().all(|g| !g))
            );
            wait_enter("Every device emits again").await;
            ctx.send(Synchronize).await
        }
        .await
    }
    .await;
    ctx.send(Clear).await?;
    ctx.stop_output().await?;
    outcome
}
