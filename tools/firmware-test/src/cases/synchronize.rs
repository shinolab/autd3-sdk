use core::time::Duration;
use std::time::Instant;

use anyhow::Result;

use autd3_rs::commands::{Clear, CpuConfig, SetCpuConfig, Synchronize};
use autd3_rs::value::{ModulationBank, TransitionMode};

use crate::Ctx;
use crate::cases::cpu_config::silence;
use crate::cases::pattern_util::{activate_mod_bank, resyncs};

const CYCLE_NS: u64 = 1_000_000;
const AHEAD: Duration = Duration::from_millis(300);
const POLL_WINDOW: Duration = Duration::from_millis(800);
const TOLERANCE_US: i64 = 500;

struct Bracket {
    still_b0_us: i64,
    seen_b1_us: Option<i64>,
}

fn micros_from(target: Instant, at: Instant) -> i64 {
    if at >= target {
        i64::try_from((at - target).as_micros()).unwrap_or(i64::MAX)
    } else {
        -i64::try_from((target - at).as_micros()).unwrap_or(i64::MAX)
    }
}

async fn synchronize_at_phase(ctx: &Ctx<'_>, phase_ns: Option<u64>) -> Result<(u64, Duration)> {
    if let Some(phase_ns) = phase_ns {
        let deadline = Instant::now() + Duration::from_millis(5);
        while Instant::now() < deadline {
            let now = ctx.client.device_time_now()?.sys_time() % CYCLE_NS;
            if now >= phase_ns && now < phase_ns + 20_000 {
                break;
            }
            std::hint::spin_loop();
        }
    }
    let sent_phase = ctx.client.device_time_now()?.sys_time() % CYCLE_NS;
    let started = Instant::now();
    ctx.send(Synchronize).await?;
    Ok((sent_phase, started.elapsed()))
}

async fn transition_brackets(ctx: &Ctx<'_>) -> Result<Vec<Bracket>> {
    activate_mod_bank(ctx, ModulationBank::B0, TransitionMode::Immediate).await?;
    let target = Instant::now() + AHEAD;
    let time = ctx.client.device_time_now()? + AHEAD;
    activate_mod_bank(ctx, ModulationBank::B1, TransitionMode::SysTime { time }).await?;

    let mut brackets: Vec<Bracket> = (0..ctx.client.num_devices())
        .map(|_| Bracket {
            still_b0_us: micros_from(target, Instant::now()),
            seen_b1_us: None,
        })
        .collect();
    let deadline = target + POLL_WINDOW;
    while Instant::now() < deadline && brackets.iter().any(|b| b.seen_b1_us.is_none()) {
        let before = Instant::now();
        let states = ctx.client.read_fpga_state().await?;
        let after = Instant::now();
        for (bracket, state) in brackets.iter_mut().zip(&states) {
            if bracket.seen_b1_us.is_some() {
                continue;
            }
            if state.current_mod_bank() == ModulationBank::B1 {
                bracket.seen_b1_us = Some(micros_from(target, after));
            } else {
                bracket.still_b0_us = micros_from(target, before);
            }
        }
    }
    activate_mod_bank(ctx, ModulationBank::B0, TransitionMode::Immediate).await?;
    Ok(brackets)
}

fn report(label: &str, brackets: &[Bracket]) -> bool {
    let mut ok = true;
    let cells: Vec<String> = brackets
        .iter()
        .map(|b| {
            if let Some(hi) = b.seen_b1_us {
                if b.still_b0_us > TOLERANCE_US || hi < -TOLERANCE_US {
                    ok = false;
                }
                format!("({:+}, {:+}]", b.still_b0_us, hi)
            } else {
                ok = false;
                format!("({:+}, never]", b.still_b0_us)
            }
        })
        .collect();
    println!(
        "  [{}] {label}: transition - target [us] {}",
        if ok { "OK" } else { "FAIL" },
        cells.join(" ")
    );
    ok
}

async fn phase_sweep(ctx: &Ctx<'_>, rounds: usize) -> Result<usize> {
    println!("-- default guard, Synchronize sent at a chosen phase of the 1 ms cycle");
    let mut failures = 0;
    for _ in 0..rounds {
        for phase_ns in [50_000, 250_000, 450_000, 650_000, 700_000, 800_000, 900_000] {
            let (sent_phase, elapsed) = synchronize_at_phase(ctx, Some(phase_ns)).await?;
            tokio::time::sleep(Duration::from_millis(20)).await;
            let brackets = transition_brackets(ctx).await?;
            if !report(
                &format!(
                    "sent at phase {:>3} us, accepted in {elapsed:?}",
                    sent_phase / 1000
                ),
                &brackets,
            ) {
                failures += 1;
            }
        }
    }
    Ok(failures)
}

async fn guard_sweep(ctx: &Ctx<'_>) -> Result<usize> {
    println!("-- other guards");
    let mut failures = 0;
    for guard in [
        Duration::ZERO,
        Duration::from_millis(2),
        Duration::from_millis(5),
    ] {
        ctx.send(SetCpuConfig::new(CpuConfig {
            sync_guard: guard,
            ..CpuConfig::default()
        }))
        .await?;
        for _ in 0..3 {
            let (_, elapsed) = synchronize_at_phase(ctx, None).await?;
            tokio::time::sleep(Duration::from_millis(20)).await;
            let brackets = transition_brackets(ctx).await?;
            if !report(
                &format!("guard {guard:?}, accepted in {elapsed:?}"),
                &brackets,
            ) {
                failures += 1;
            }
        }
    }
    ctx.send(SetCpuConfig::new(CpuConfig::default())).await?;
    ctx.send(Synchronize).await?;
    Ok(failures)
}

async fn repeated(ctx: &Ctx<'_>, count: usize) -> Result<usize> {
    println!("-- {count} Synchronize in a row at the default guard");
    let mut failures = 0;
    let mut slowest = Duration::ZERO;
    for _ in 0..count {
        match synchronize_at_phase(ctx, None).await {
            Ok((_, elapsed)) => slowest = slowest.max(elapsed),
            Err(e) => {
                failures += 1;
                println!("  [FAIL] Synchronize: {e:#}");
            }
        }
    }
    tokio::time::sleep(Duration::from_millis(1500)).await;
    println!(
        "  [{}] {failures} failure(s), slowest {slowest:?}, SyncResync {:?}",
        if failures == 0 { "OK" } else { "FAIL" },
        resyncs(ctx).await?
    );
    Ok(failures)
}

pub async fn run(ctx: &Ctx<'_>) -> Result<()> {
    silence(ctx).await?;
    let outcome = async {
        let mut failures = phase_sweep(ctx, 3).await?;
        failures += guard_sweep(ctx).await?;
        failures += repeated(ctx, 200).await?;
        failures += usize::from(!report(
            "after the repeated Synchronize",
            &transition_brackets(ctx).await?,
        ));
        println!("  {failures} failing check(s) in total");
        anyhow::Ok(())
    }
    .await;
    ctx.send(Clear).await?;
    ctx.client.silent_stop().await?;
    outcome
}
