use core::time::Duration;
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result, bail};

use autd3_rs::commands::{Clear, CpuConfig, ReleaseFailsafe, SetCpuConfig, Synchronize};
use autd3_rs::{DeviceState, Frames};

use crate::cases::cpu_config::{silence, states};
use crate::cases::pattern_util::expect_firmware_ok;
use crate::io::prompt;
use crate::{Ctx, Link, Session, failsafe_gates};

const TIMEOUT: Duration = Duration::from_secs(1);
const LONGER_TIMEOUT: Duration = Duration::from_secs(3);
const SHORT_HALT: Duration = Duration::from_millis(500);
const LONG_HALT: Duration = Duration::from_secs(2);
const HALT_LOST_TIMEOUT: Duration = Duration::from_secs(5);
const RELOCK: Duration = Duration::from_secs(10);
const JLINK_CANDIDATES: &[&str] = &["JLinkExe", "JLink"];

fn verdict(ok: bool) -> &'static str {
    if ok { "[OK]" } else { "[FAIL]" }
}

struct Observed {
    watchdog: Vec<u8>,
    host_trips: Vec<u16>,
    gates: Vec<bool>,
}

async fn observe(ctx: &Ctx<'_>) -> Result<Observed> {
    let telemetry = ctx.client.read_telemetry().await?;
    Ok(Observed {
        watchdog: telemetry.iter().map(|t| t.cpu_watchdog).collect(),
        host_trips: telemetry.iter().map(|t| t.failsafe).collect(),
        gates: failsafe_gates(ctx.client)?,
    })
}

fn fired(before: &[u8], after: &[u8]) -> Vec<u8> {
    before
        .iter()
        .zip(after)
        .map(|(b, a)| a.wrapping_sub(*b))
        .collect()
}

fn only_target(values: &[bool], target: usize) -> bool {
    values.iter().enumerate().all(|(i, v)| *v == (i == target))
}

async fn jlink(commands: String) -> Result<()> {
    fn run_jlink(commands: &str) -> Result<()> {
        let script =
            std::env::temp_dir().join(format!("autd3-firmware-test-{}.jlink", std::process::id()));
        std::fs::write(&script, commands)
            .with_context(|| format!("writing {}", script.display()))?;
        let programs: Vec<String> = match std::env::var("JLINK") {
            Ok(v) if !v.is_empty() => vec![v],
            _ => JLINK_CANDIDATES.iter().map(|p| (*p).to_string()).collect(),
        };
        let mut outcome = None;
        for program in &programs {
            let spawned = Command::new(program)
                .args([
                    "-device",
                    "R7S910018_R4F",
                    "-if",
                    "JTAG",
                    "-speed",
                    "4000",
                    "-jtagconf",
                    "-1,-1",
                    "-autoconnect",
                    "1",
                    "-ExitOnError",
                    "1",
                    "-CommanderScript",
                ])
                .arg(&script)
                .output();
            match spawned {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                result => {
                    outcome = Some(result.with_context(|| format!("failed to run `{program}`"))?);
                    break;
                }
            }
        }
        let _ = std::fs::remove_file(&script);
        let Some(output) = outcome else {
            bail!("J-Link Commander (JLinkExe) not found on PATH (install J-Link or set JLINK)");
        };
        if !output.status.success() {
            bail!(
                "J-Link exited with {}:\n{}{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    tokio::task::spawn_blocking(move || run_jlink(&commands)).await?
}

async fn config(ctx: &Ctx<'_>, cpu_watchdog_timeout: Option<Duration>) -> Result<()> {
    ctx.send(SetCpuConfig::new(CpuConfig {
        cpu_watchdog_timeout,
        ..CpuConfig::default()
    }))
    .await
}

async fn halted(ctx: &Ctx<'_>, label: &str, halt: Duration) -> Result<(Observed, Vec<u8>)> {
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

    let before = observe(ctx).await?;
    jlink(format!("h\nsleep {}\ng\nq\n", halt.as_millis())).await?;
    let (ready, elapsed) = all_ready_within(ctx, RELOCK).await;
    println!(
        "  {label}: halted for {halt:?}; {} every device answers and is locked again after {:.1} s: {:?}",
        verdict(ready),
        elapsed.as_secs_f32(),
        states(ctx)
    );
    let after = observe(ctx).await?;
    let fired = fired(&before.watchdog, &after.watchdog);
    Ok((after, fired))
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn run(client: Session, link: &Link<'_>) -> Result<Session> {
    async fn target(devices: usize) -> Option<usize> {
        let answer = prompt(&format!(
            "Index of the device the J-Link is attached to (0..{devices}, default 0)"
        ))
        .await;
        let answer = answer.trim();
        if answer.is_empty() {
            return Some(0);
        }
        answer.parse::<usize>().ok().filter(|i| *i < devices)
    }

    #[allow(clippy::too_many_lines)]
    async fn halts(ctx: &Ctx<'_>, target: usize) -> Result<()> {
        async fn below_timeout(ctx: &Ctx<'_>) -> Result<()> {
            println!("-- a halt shorter than the timeout");
            config(ctx, Some(TIMEOUT)).await?;
            let (seen, fired) = halted(ctx, "timeout 1 s", SHORT_HALT).await?;
            println!(
                "  {} no device fires: fired {fired:?} gates {:?}",
                verdict(fired.iter().all(|c| *c == 0) && seen.gates.iter().all(|g| !g)),
                seen.gates
            );
            Ok(())
        }

        async fn above_timeout(ctx: &Ctx<'_>, target: usize) -> Result<()> {
            println!("-- a halt longer than the timeout");
            config(ctx, Some(TIMEOUT)).await?;
            let (seen, fired) = halted(ctx, "timeout 1 s", LONG_HALT).await?;
            println!(
                "  {} only device[{target}] fires, once: fired {fired:?}",
                verdict(
                    fired
                        .iter()
                        .enumerate()
                        .all(|(i, c)| *c == u8::from(i == target))
                )
            );
            println!(
                "  {} the gate is up on device[{target}] alone: gates {:?}",
                verdict(only_target(&seen.gates, target)),
                seen.gates
            );
            println!(
                "  {} the disabled host-silence failsafe did not trip, so the gate is the watchdog's: host trips {:?}",
                verdict(seen.host_trips.iter().all(|c| *c == 0)),
                seen.host_trips
            );

            tokio::time::sleep(LONG_HALT).await;
            let later = observe(ctx).await?;
            println!(
                "  {} the gate stays up after the CPU runs again (no automatic release): gates {:?}",
                verdict(only_target(&later.gates, target)),
                later.gates
            );

            expect_firmware_ok(
                ctx,
                "ReleaseFailsafe after the watchdog fired",
                Frames::encode(ctx.client.geometry(), ReleaseFailsafe),
            )
            .await;
            let released = observe(ctx).await?;
            println!(
                "  {} the release lowered the gate: gates {:?}",
                verdict(released.gates.iter().all(|g| !g)),
                released.gates
            );
            println!(
                "  {} the release keeps the count: {:?} -> {:?}",
                verdict(released.watchdog == seen.watchdog),
                seen.watchdog,
                released.watchdog
            );
            Ok(())
        }

        async fn longer_timeout(ctx: &Ctx<'_>) -> Result<()> {
            println!("-- the configured timeout is the one the FPGA uses");
            config(ctx, Some(LONGER_TIMEOUT)).await?;
            let (seen, fired) = halted(ctx, "timeout 3 s", LONG_HALT).await?;
            println!(
                "  {} no device fires: fired {fired:?} gates {:?}",
                verdict(fired.iter().all(|c| *c == 0) && seen.gates.iter().all(|g| !g)),
                seen.gates
            );
            Ok(())
        }

        async fn disabled(ctx: &Ctx<'_>) -> Result<()> {
            println!("-- a disabled watchdog");
            config(ctx, None).await?;
            let (seen, fired) = halted(ctx, "timeout None", LONG_HALT).await?;
            println!(
                "  {} no device fires: fired {fired:?} gates {:?}",
                verdict(fired.iter().all(|c| *c == 0) && seen.gates.iter().all(|g| !g)),
                seen.gates
            );
            Ok(())
        }

        if ctx.lost_timeout < HALT_LOST_TIMEOUT {
            println!(
                "  [SKIP] the halted device would turn Lost after {:?} and Lost needs a reopen, which lowers the gate; rerun with --lost-timeout-ms {} or more",
                ctx.lost_timeout,
                HALT_LOST_TIMEOUT.as_millis()
            );
            return Ok(());
        }
        ctx.send(Clear).await?;
        silence(ctx).await?;
        let outcome = async {
            below_timeout(ctx).await?;
            above_timeout(ctx, target).await?;
            longer_timeout(ctx).await?;
            disabled(ctx).await
        }
        .await;
        ctx.send(Clear).await?;
        ctx.send(Synchronize).await?;
        ctx.stop_output().await?;
        outcome
    }

    let Some(target) = target(client.num_devices()).await else {
        println!("  [SKIP] not a device index");
        return Ok(client);
    };
    let ctx = Ctx {
        client: &client,
        geometry: link.geometry,
        heartbeat: link.heartbeat,
        lost_timeout: link.option.lost_timeout,
        mute: link.mute,
        exec: link.exec,
    };
    if let Err(e) = halts(&ctx, target).await {
        println!("  [FAIL] the halt checks stopped: {e:#}");
    }
    println!("-- a CPU reset longer than the timeout");
    let before = client.read_telemetry().await?;
    if let Err(e) = client.close() {
        println!("  closing the client before the reset reported: {e}");
    }
    drop(client);
    let started = Instant::now();
    let held = jlink(format!("r\nsleep {}\ng\nq\n", LONG_HALT.as_millis())).await;
    if let Err(e) = &held {
        println!("  [FAIL] holding device[{target}] in reset: {e:#}");
    }
    let client = link.reopen().await?;
    println!(
        "  every device enumerates again {:.1} s after the reset began",
        started.elapsed().as_secs_f32()
    );
    held?;
    let after = client.read_telemetry().await?;
    let before: Vec<u8> = before.iter().map(|t| t.cpu_watchdog).collect();
    let after: Vec<u8> = after.iter().map(|t| t.cpu_watchdog).collect();
    let fired = fired(&before, &after);
    println!(
        "  {} only device[{target}] fires while its CPU is held in reset, and the count survives the reset: fired {fired:?}",
        verdict(
            fired
                .iter()
                .enumerate()
                .all(|(i, c)| *c == u8::from(i == target))
        )
    );
    let gates = failsafe_gates(&client)?;
    println!(
        "  {} the gate is down after the reboot: gates {gates:?}",
        verdict(gates.iter().all(|g| !g))
    );
    Ok(client)
}
