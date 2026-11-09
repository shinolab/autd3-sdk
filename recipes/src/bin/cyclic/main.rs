mod controller;
mod report;

use std::num::NonZeroUsize;
use std::pin::pin;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs::commands::{Clear, Pattern, Synchronize};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Intensity, Phase};
use autd3_rs::{Controller, Interface, TransportOption};
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::controller::CyclicController;
use crate::report::{LiveBlock, Stats, TelemetryCounters, print_summary};

#[derive(Parser)]
#[command(about = "Process a frame on every device at a fixed period")]
struct Cli {
    #[arg(
        long,
        help = "Network interface the devices hang off. Omit to pick the one whose devices answer."
    )]
    interface: Option<String>,
    #[arg(long, default_value_t = NonZeroUsize::MIN, help = "Device count of the geometry")]
    devices: NonZeroUsize,
    #[arg(long, value_parser = humantime::parse_duration, default_value = "1ms", help = "Period at which the devices process a frame")]
    period: Duration,
    #[arg(long, value_parser = humantime::parse_duration, default_value = "2ms", help = "How long before its processing time each frame is sent")]
    lead: Duration,
}

#[cfg(target_os = "linux")]
fn raise_priority() -> Result<()> {
    use thread_priority::{
        RealtimeThreadSchedulePolicy, ThreadPriority, ThreadPriorityValue, ThreadSchedulePolicy,
        set_thread_priority_and_policy, thread_native_id,
    };
    let priority = ThreadPriority::Crossplatform(
        ThreadPriorityValue::try_from(80u8).map_err(|e| anyhow::anyhow!(e))?,
    );
    set_thread_priority_and_policy(
        thread_native_id(),
        priority,
        ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo),
    )
    .context("switching to SCHED_FIFO failed (run with sudo)")
}

#[cfg(target_os = "windows")]
fn raise_priority() -> Result<()> {
    use thread_priority::{ThreadPriority, ThreadPriorityValue, set_current_thread_priority};
    let priority = ThreadPriority::Crossplatform(
        ThreadPriorityValue::try_from(99u8).map_err(|e| anyhow::anyhow!(e))?,
    );
    set_current_thread_priority(priority).context("raising the thread priority to TimeCritical")
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    raise_priority()?;
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    tracing::warn!(
        "raising the priority is not implemented on this OS; running at the normal priority"
    );
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(cli))
}

async fn run(cli: Cli) -> Result<()> {
    let num_devices = cli.devices.get();
    let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()));
    let option = TransportOption {
        iface: cli.interface.map_or(Interface::Auto, Interface::Name),
        ..TransportOption::default()
    };
    let controller = Controller::open(&geometry, &option)
        .await
        .context("opening the devices")?;
    controller.send(Clear).await.context("initial Clear")?;
    controller
        .send(Synchronize)
        .await
        .context("initial Synchronize")?;
    let firmware = controller
        .read_firmware_version()
        .await
        .context("reading firmware version")?;
    for (i, firmware) in firmware.iter().enumerate() {
        eprintln!("device[{i}] firmware version: {firmware}");
    }
    eprintln!(
        "period {:?}, lead {:?}; press Ctrl+C to stop",
        cli.period, cli.lead
    );

    let (cyclic, mut reports) = CyclicController::start(controller, cli.period, cli.lead)?;
    let started = Instant::now();
    let mut total = Stats::new(num_devices);
    let mut interval = Stats::new(num_devices);
    let mut telemetry = TelemetryCounters::new(num_devices);
    let mut live = LiveBlock::new();
    let outcome = {
        let mut phases = geometry.phase_buffer();
        let mut load = pin!(async {
            let mut tick = 0u8;
            loop {
                for phase in phases.iter_mut().flatten() {
                    *phase = Phase(tick);
                }
                if let Err(e) = cyclic.send(Pattern::new(&phases, Intensity::MIN)).await {
                    return e;
                }
                tick = tick.wrapping_add(1);
            }
        });
        let mut ctrl_c = pin!(tokio::signal::ctrl_c());
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.tick().await;
        let mut last_draw = started;
        loop {
            tokio::select! {
                report = reports.recv() => {
                    let Some(report) = report else { break Ok(()) };
                    total.record(&report);
                    interval.record(&report);
                    telemetry.record(&report);
                }
                _ = ticker.tick() => {
                    let now = Instant::now();
                    live.draw(now - started, now - last_draw, &interval, &total, &telemetry);
                    last_draw = now;
                    interval = Stats::new(num_devices);
                }
                e = &mut load => break Err(e),
                _ = &mut ctrl_c => break Ok(()),
            }
        }
    };
    let elapsed = started.elapsed();
    let stopped = cyclic.stop().await;
    print_summary(elapsed, cli.period, cli.lead, &total, &telemetry);
    stopped.and(outcome)
}
