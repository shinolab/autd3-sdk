use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::operation::{ConfigPattern, WritePatternBuffer};
use autd3_rs::params::NUM_TRANSDUCERS;
use autd3_rs::value::{
    Emission, Intensity, LoopBehavior, PatternBank, PatternDataType, Phase, SamplingConfig,
};
use autd3_rs::{
    Client, ClientConfig, Datagrams, Error as ClientError, Link, ResponseFuture, StateCheck,
};
use autd3_rs_link_ethercrab::{EtherCrabLink, EtherCrabLinkOption};
use autd3_rs_link_soem::{SoemLink, SoemLinkOption};

use crate::cli::{Cli, LinkKind, Mode};

const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
const STATE_CHECK_INTERVAL: Duration = Duration::from_millis(100);

struct StateCheckGuard {
    stop: Arc<AtomicBool>,
    join: tokio::task::JoinHandle<()>,
}

impl StateCheckGuard {
    async fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.join.await;
    }
}

fn spawn_state_check<C: StateCheck>(mut checker: C, interval: Duration) -> StateCheckGuard {
    let stop = Arc::new(AtomicBool::new(false));
    let join = tokio::spawn({
        let stop = Arc::clone(&stop);
        async move {
            while !stop.load(Ordering::Relaxed) {
                if Box::pin(checker.check()).await.is_err() {
                    break;
                }
                tokio::time::sleep(interval).await;
            }
        }
    });
    StateCheckGuard { stop, join }
}

pub async fn run(cli: &Cli) -> Result<()> {
    match cli.link {
        LinkKind::Ethercrab => {
            let link_cfg = EtherCrabLinkOption {
                interface: cli.interface.clone().into(),
                sync0_period: Duration::from_micros(cli.cycle_us),
                ..Default::default()
            };
            let link = Box::pin(EtherCrabLink::open(link_cfg))
                .await
                .context("opening EtherCAT link (ethercrab)")?;
            let guard = spawn_state_check(link.state_checker(), STATE_CHECK_INTERVAL);
            let out = Box::pin(run_with_link(link, cli)).await;
            guard.stop().await;
            out
        }
        LinkKind::Soem => {
            let link_cfg = SoemLinkOption {
                interface: cli.interface.clone().into(),
                sync0_period: Duration::from_micros(cli.cycle_us),
                ..Default::default()
            };
            let link = tokio::task::spawn_blocking(move || SoemLink::open(link_cfg))
                .await
                .expect("open task panicked")
                .context("opening EtherCAT link (SOEM)")?;
            let guard = spawn_state_check(link.state_checker(), STATE_CHECK_INTERVAL);
            let out = Box::pin(run_with_link(link, cli)).await;
            guard.stop().await;
            out
        }
    }
}

async fn run_with_link<L: Link>(link: L, cli: &Cli) -> Result<()> {
    let num_devices = link.num_devices();
    eprintln!("devices: {num_devices}");
    if let Some(expected) = cli.devices
        && num_devices != expected
    {
        anyhow::bail!("expected {expected} device(s) on the bus, found {num_devices}");
    }

    let max_inflight = match cli.mode {
        Mode::StopAndWait => 1,
        Mode::Streaming => cli.inflight.max(1),
    };
    let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()).collect());
    let client = Box::pin(Client::open(
        &geometry,
        link,
        ClientConfig {
            timeout_cycles: cli.timeout_cycles,
            max_inflight: NonZeroUsize::new(max_inflight).unwrap(),
            send_interval_cycles: cli.send_interval_cycles,
            max_resync_rounds: cli.max_resync_rounds,
            low_latency: cli.low_latency,
            reset_resend_cycles: 2,
            rt_priority: None,
            rt_affinity: None,
            validate_state: false,
        },
    ))
    .await
    .context("client handshake")?;

    let fw = client
        .read_firmware_version()
        .await
        .context("reading firmware version")?;
    for (i, fw) in fw.iter().enumerate() {
        eprintln!("device[{i}] firmware version: {fw}");
    }

    send_config_pattern_once(&client)
        .await
        .context("initial ConfigPattern")?;

    let shutdown = Arc::new(AtomicBool::new(false));
    spawn_signal_listener(Arc::clone(&shutdown));

    eprintln!("sending WritePatternBuffer continuously — press Ctrl+C to stop");
    let result = match cli.mode {
        Mode::StopAndWait => soak_stop_and_wait(&client, &geometry, cli, shutdown).await,
        Mode::Streaming => soak_streaming(&client, &geometry, cli, shutdown, max_inflight).await,
    };

    let _ = client.close().await;
    result
}

async fn send_config_pattern_once(client: &Client) -> Result<()> {
    let mut builder = client.datagram_builder();
    builder.push(ConfigPattern {
        bank: PatternBank::B0,
        divider: SamplingConfig::FREQ_4K.divide().unwrap_or(1),
        size: 1,
        data_type: PatternDataType::Raw,
        rep: LoopBehavior::Infinite.rep(),
    });
    let datagrams = builder.build()?;
    for frame in &datagrams {
        client.send_checked(frame).await?;
    }
    Ok(())
}

async fn soak_stop_and_wait(
    client: &Client,
    geometry: &Geometry,
    cli: &Cli,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    let mut emissions = geometry.pattern_buffer();
    let mut datagrams = Datagrams::default();
    let mut progress = Progress::new(cli);

    let mut tick: u8 = 0;
    let mut index: u64 = 0;
    let start = Instant::now();
    loop {
        if should_stop(&shutdown, cli, index, start) {
            break;
        }

        fill_emissions(&mut emissions, tick);
        encode_write(client, &emissions, &mut datagrams)?;
        let res = client
            .send_checked(datagrams.frame(0).expect("one frame"))
            .await;

        match res {
            Ok(()) => progress.observe_ok(),
            Err(e) => {
                if progress.observe_error(classify(&e)) {
                    eprintln!("\nsend #{index} failed: {e}");
                }
                if cli.stop_on_error {
                    progress.finish(start.elapsed());
                    return Err(anyhow::anyhow!("send #{index} failed: {e}"));
                }
            }
        }

        progress.maybe_render(index + 1, start.elapsed());
        tick = tick.wrapping_add(1);
        index += 1;
    }

    progress.finish(start.elapsed());
    Ok(())
}

async fn soak_streaming(
    client: &Client,
    geometry: &Geometry,
    cli: &Cli,
    shutdown: Arc<AtomicBool>,
    max_inflight: usize,
) -> Result<()> {
    let mut emissions = geometry.pattern_buffer();
    let mut datagrams = Datagrams::default();
    let mut pending: VecDeque<ResponseFuture> = VecDeque::with_capacity(max_inflight);
    let mut progress = Progress::new(cli);

    let mut tick: u8 = 0;
    let mut sends_issued: u64 = 0;
    let mut completed: u64 = 0;
    let start = Instant::now();
    loop {
        let need_send = !should_stop(&shutdown, cli, sends_issued, start);

        if need_send && pending.len() < max_inflight {
            fill_emissions(&mut emissions, tick);
            encode_write(client, &emissions, &mut datagrams)?;
            let fut = client.send(datagrams.frame(0).expect("one frame")).await?;
            pending.push_back(fut);
            sends_issued += 1;
            tick = tick.wrapping_add(1);
            continue;
        }

        let Some(fut) = pending.pop_front() else {
            break;
        };
        let res = fut.await.and_then(|r| r.check());
        match res {
            Ok(()) => progress.observe_ok(),
            Err(e) => {
                if progress.observe_error(classify(&e)) {
                    eprintln!("\nsend #{completed} failed: {e}");
                }
                if cli.stop_on_error {
                    progress.finish(start.elapsed());
                    return Err(anyhow::anyhow!("send #{completed} failed: {e}"));
                }
            }
        }
        completed += 1;
        progress.maybe_render(completed, start.elapsed());
    }

    progress.finish(start.elapsed());
    Ok(())
}

fn should_stop(shutdown: &AtomicBool, cli: &Cli, issued: u64, start: Instant) -> bool {
    if shutdown.load(Ordering::Relaxed) {
        return true;
    }
    if let Some(n) = cli.count
        && issued >= n
    {
        return true;
    }
    if let Some(d) = cli.duration
        && start.elapsed() >= d
    {
        return true;
    }
    false
}

fn encode_write(
    client: &Client,
    emissions: &[[Emission; NUM_TRANSDUCERS]],
    datagrams: &mut Datagrams,
) -> Result<()> {
    let mut builder = client.datagram_builder();
    builder.push(WritePatternBuffer {
        bank: PatternBank::B0,
        index: 0,
        emissions,
    });
    builder
        .build_into(datagrams)
        .context("encoding WritePatternBuffer")?;
    Ok(())
}

fn fill_emissions(emissions: &mut [[Emission; NUM_TRANSDUCERS]], tick: u8) {
    for device in emissions {
        let mut phase = tick;
        for e in device.iter_mut() {
            e.phase = Phase(phase);
            e.intensity = Intensity::MIN;
            phase = phase.wrapping_add(1);
        }
    }
}

#[derive(Clone, Copy)]
enum ErrorKind {
    Timeout,
    Device,
    Link,
}

fn classify(e: &ClientError) -> ErrorKind {
    match e {
        ClientError::Timeout { .. } => ErrorKind::Timeout,
        ClientError::DeviceError { .. } => ErrorKind::Device,
        _ => ErrorKind::Link,
    }
}

struct Progress {
    count_total: Option<u64>,
    duration_total: Option<Duration>,
    ok: u64,
    timeouts: u64,
    device_errors: u64,
    link_errors: u64,
    first_error_logged: bool,
    last_render: Instant,
    rendered_once: bool,
}

impl Progress {
    fn new(cli: &Cli) -> Self {
        Self {
            count_total: cli.count,
            duration_total: cli.duration,
            ok: 0,
            timeouts: 0,
            device_errors: 0,
            link_errors: 0,
            first_error_logged: false,
            last_render: Instant::now()
                .checked_sub(PROGRESS_INTERVAL)
                .unwrap_or_else(Instant::now),
            rendered_once: false,
        }
    }

    fn observe_ok(&mut self) {
        self.ok += 1;
    }

    fn observe_error(&mut self, kind: ErrorKind) -> bool {
        match kind {
            ErrorKind::Timeout => self.timeouts += 1,
            ErrorKind::Device => self.device_errors += 1,
            ErrorKind::Link => self.link_errors += 1,
        }
        if self.first_error_logged {
            false
        } else {
            self.first_error_logged = true;
            true
        }
    }

    fn maybe_render(&mut self, completed: u64, elapsed: Duration) {
        let now = Instant::now();
        if now.duration_since(self.last_render) >= PROGRESS_INTERVAL {
            self.render(completed, elapsed);
            self.last_render = now;
        }
    }

    fn render(&mut self, completed: u64, elapsed: Duration) {
        let progress_field = if let Some(total) = self.count_total {
            format!("{completed:>10}/{total}")
        } else if let Some(total) = self.duration_total {
            format!("{:>6.1}/{:.1}s", elapsed.as_secs_f64(), total.as_secs_f64())
        } else {
            format!("{completed:>10} ({:.1}s)", elapsed.as_secs_f64())
        };
        eprint!(
            "\r[{progress_field}] ok={} timeout={} dev_err={} link_err={}    ",
            self.ok, self.timeouts, self.device_errors, self.link_errors,
        );
        let _ = std::io::Write::flush(&mut std::io::stderr());
        self.rendered_once = true;
    }

    fn finish(&mut self, elapsed: Duration) {
        let total = self.ok + self.timeouts + self.device_errors + self.link_errors;
        self.render(total, elapsed);
        if self.rendered_once {
            eprintln!();
        }
        eprintln!(
            "done: sent={total} ok={} timeout={} dev_err={} link_err={} in {:.1}s",
            self.ok,
            self.timeouts,
            self.device_errors,
            self.link_errors,
            elapsed.as_secs_f64(),
        );
    }
}

fn spawn_signal_listener(flag: Arc<AtomicBool>) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            flag.store(true, Ordering::Relaxed);
            eprintln!("\nCtrl+C received — stopping after the current send...");
        }
    });
}
