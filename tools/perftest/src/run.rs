use std::collections::VecDeque;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs::commands::{ConfigPattern, GpioOut, Nop, Pattern, SetGpioOut, WritePatternBuffer};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::protocol::TX_FRAME_BYTES;
use autd3_rs::value::{Emission, Intensity, LoopBehavior, PatternBank, Phase, SamplingConfig};
use autd3_rs::{
    Client, ClientConfig, CoreId, Error as ClientError, Frames, IntoLink, Link, LinkStats,
    ResponseFuture, RtSchedulePolicy, StateCheck, ThreadPriority, ThreadPriorityValue,
};
use autd3_rs_link_ethercrab::{EtherCrabLink, EtherCrabLinkOption};
use autd3_rs_link_remote::{DiscoveryOption, RemoteLink, discover};

use autd3_rs_link_soem::{SoemLink, SoemLinkOption};
use autd3_rs_link_twincat::{TwinCATLink, TwinCATLinkOption};

use crate::cli::{Cli, Command, LinkKind, Mode, RtPolicy};
use crate::mem::{self, MemProfile};
use crate::nop::PacedNop;
use crate::stats::{Sample, SampleStatus};

const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
const STATE_CHECK_INTERVAL: Duration = Duration::from_millis(100);

pub struct RunOutput {
    pub samples: Vec<Sample>,
    pub sends: u64,
    pub stopped_on_error: Option<(u64, SampleStatus)>,
    pub rt_closed: bool,
    pub warmup: u64,
    pub elapsed: Duration,
    pub frame_bytes: usize,
    pub stale_cycles: u64,
    pub lost_cycles: u64,
    pub mem: Option<MemProfile>,
}

struct Sender {
    command: Command,
    frames: Frames,
    emissions: Vec<Vec<Emission>>,
    tick: u8,
}

impl Sender {
    fn new(client: &Client, geometry: &Geometry, cli: &Cli) -> Result<Self> {
        let mut sender = Self {
            command: cli.command,
            frames: Frames::default(),
            emissions: if cli.command.is_pattern() {
                geometry.pattern_buffer()
            } else {
                Vec::new()
            },
            tick: 0,
        };
        if cli.command == Command::Nop {
            let mut builder = client.datagram_builder();
            builder.push(Nop);
            builder
                .build_into(&mut sender.frames)
                .context("building Nop frame")?;
        }
        Ok(sender)
    }

    fn prepare(&mut self, client: &Client) -> Result<()> {
        if self.command == Command::Nop {
            return Ok(());
        }
        fill_emissions(&mut self.emissions, self.tick);
        self.tick = self.tick.wrapping_add(1);

        let mut builder = client.datagram_builder();
        if self.command == Command::Pattern {
            builder.push(Pattern::with_bank(PatternBank::B0, &self.emissions));
        } else {
            builder.push(WritePatternBuffer {
                bank: PatternBank::B0,
                index: 0,
                emissions: &self.emissions,
            });
        }
        builder
            .build_into(&mut self.frames)
            .context("encoding pattern write")?;
        Ok(())
    }
}

fn fill_emissions(emissions: &mut [Vec<Emission>], tick: u8) {
    for device in emissions {
        let mut phase = tick;
        for e in device.iter_mut() {
            e.phase = Phase(phase);
            e.intensity = Intensity::MIN;
            phase = phase.wrapping_add(1);
        }
    }
}

async fn send_config_pattern_once(client: &Client) -> Result<()> {
    let mut builder = client.datagram_builder();
    builder.push(ConfigPattern {
        bank: PatternBank::B0,
        config: SamplingConfig::FREQ_4K,
        size: 1,
        loop_behavior: LoopBehavior::Infinite,
    });
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    Ok(())
}

async fn send_set_gpio_out_once(client: &Client) -> Result<()> {
    let mut builder = client.datagram_builder();
    builder.push(SetGpioOut {
        outputs: [
            GpioOut::BaseSignal,
            GpioOut::Off,
            GpioOut::Off,
            GpioOut::Off,
        ],
    });
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    Ok(())
}

struct Recorder {
    samples: Vec<Sample>,
    limit: Option<u64>,
    sends: u64,
}

impl Recorder {
    fn new(cli: &Cli) -> Self {
        let limit = (cli.max_samples != 0).then_some(cli.max_samples);
        let cap = match (estimate_capacity(cli), limit) {
            (n, Some(limit)) => n.min(usize::try_from(limit).unwrap_or(usize::MAX)),
            (n, None) => n,
        };
        Self {
            samples: Vec::with_capacity(cap),
            limit,
            sends: 0,
        }
    }

    fn push(&mut self, sample: Sample) {
        self.sends += 1;
        if self
            .limit
            .is_none_or(|limit| (self.samples.len() as u64) < limit)
        {
            self.samples.push(sample);
        }
    }
}

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
                if checker.check().await.is_err() {
                    break;
                }
                tokio::time::sleep(interval).await;
            }
        }
    });
    StateCheckGuard { stop, join }
}

pub async fn run(cli: &Cli) -> Result<RunOutput> {
    match cli.link {
        LinkKind::Ethercrab => {
            let link_cfg = EtherCrabLinkOption {
                iface: cli.interface.clone().into(),
                sync0_period: cli.sync0_period,
                sync0_shift: cli.sync0_shift(),
                ..Default::default()
            };
            let link = Box::pin(EtherCrabLink::open(link_cfg))
                .await
                .context("opening EtherCAT link (ethercrab)")?;
            let guard = spawn_state_check(link.state_checker(), STATE_CHECK_INTERVAL);
            let out = Box::pin(run_with_bus_link(link, cli)).await;
            guard.stop().await;
            out
        }
        LinkKind::Echocat => {
            let link_cfg = autd3_rs_link_echocat::EchocatLinkOption {
                iface: cli.interface.clone().into(),
                sync0_period: cli.sync0_period,
                sleep_strategy: cli.echocat_sleep_strategy(),
                ..Default::default()
            };
            let link = tokio::task::spawn_blocking(move || {
                autd3_rs_link_echocat::EchocatLink::open(&link_cfg)
            })
            .await
            .expect("open task panicked")
            .context("opening EtherCAT link (echocat)")?;
            let guard = spawn_state_check(link.state_checker(), STATE_CHECK_INTERVAL);
            let out = Box::pin(run_with_bus_link(link, cli)).await;
            guard.stop().await;
            out
        }
        LinkKind::Soem => {
            let link_cfg = SoemLinkOption {
                iface: cli.interface.clone().into(),
                sync0_period: cli.sync0_period,
                sync0_shift: cli.sync0_shift(),
                ..Default::default()
            };
            let link = tokio::task::spawn_blocking(move || SoemLink::open(link_cfg))
                .await
                .expect("open task panicked")
                .context("opening EtherCAT link (SOEM)")?;
            let guard = spawn_state_check(link.state_checker(), STATE_CHECK_INTERVAL);
            let out = Box::pin(run_with_bus_link(link, cli)).await;
            guard.stop().await;
            out
        }
        LinkKind::Twincat => {
            let opt = match (cli.twincat_remote, cli.ams_net_id) {
                (Some(addr), Some(ams_net_id)) => TwinCATLinkOption::remote(addr, ams_net_id),
                _ => TwinCATLinkOption::local(),
            };
            let link = tokio::task::spawn_blocking(move || TwinCATLink::open(opt))
                .await
                .expect("open task panicked")
                .context("opening TwinCAT link")?;
            let guard = spawn_state_check(link.state_checker(), STATE_CHECK_INTERVAL);
            let out = Box::pin(run_with_bus_link(link, cli)).await;
            guard.stop().await;
            out
        }
        LinkKind::Remote => {
            let num_devices = cli.devices.expect("--devices validated for --link remote");
            let addr = if let Some(addr) = cli.addr {
                addr
            } else {
                let appliance = discover(&DiscoveryOption {
                    instance: cli.instance.clone(),
                    ..Default::default()
                })
                .context("finding the appliance over mDNS")?;
                eprintln!("appliance: {} at {}", appliance.instance, appliance.addr);
                appliance.addr
            };
            let link = tokio::task::spawn_blocking(move || {
                let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()).collect());
                RemoteLink::open(addr, None, &geometry)
            })
            .await
            .expect("open task panicked")
            .with_context(|| format!("opening the remote link to {addr}"))?;
            let guard = spawn_state_check(link.state_checker(), STATE_CHECK_INTERVAL);
            let out = Box::pin(run_with_bus_link(link, cli)).await;
            guard.stop().await;
            out
        }
        LinkKind::Nop => {
            let num_devices = cli.devices.expect("--devices validated for --link nop");
            let link = PacedNop::new(cli.sync0_period);
            Box::pin(run_with_link(link, num_devices, LinkStats::default(), cli)).await
        }
    }
}

async fn run_with_bus_link<L: Link>(link: L, cli: &Cli) -> Result<RunOutput> {
    let num_devices = link.num_devices();
    if let Some(expected) = cli.devices
        && num_devices != expected
    {
        anyhow::bail!("expected {expected} device(s) on the bus, found {num_devices}");
    }
    let link_stats = link.stats();
    run_with_link(link, num_devices, link_stats, cli).await
}

async fn run_with_link<T: IntoLink>(
    link: T,
    num_devices: usize,
    link_stats: LinkStats,
    cli: &Cli,
) -> Result<RunOutput> {
    eprintln!("devices: {num_devices}");

    let max_inflight = match cli.mode {
        Mode::StopAndWait => 1,
        Mode::Streaming => cli.max_inflight.max(1),
    };
    let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()).collect());
    let client = Box::pin(Client::open(
        &geometry,
        link,
        ClientConfig {
            timeout_cycles: cli.timeout_cycles,
            max_inflight: NonZeroUsize::new(max_inflight).unwrap(),
            max_resync_rounds: cli.max_resync_rounds,
            low_latency: cli.low_latency,
            reset_resend_cycles: NonZeroU32::new(2).unwrap(),
            rt_priority: match cli.rt_priority {
                Some(p) => Some(ThreadPriority::Crossplatform(
                    ThreadPriorityValue::try_from(p).expect("validated to 0..=99"),
                )),
                None => ClientConfig::default().rt_priority,
            },
            rt_policy: match cli.rt_policy {
                RtPolicy::Normal => RtSchedulePolicy::Normal,
                RtPolicy::Fifo => RtSchedulePolicy::Fifo,
                RtPolicy::RoundRobin => RtSchedulePolicy::RoundRobin,
            },
            rt_affinity: cli.rt_affinity.map(|id| CoreId { id }),
            validate_state: false,
            ..Default::default()
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

    if cli.command.is_pattern() {
        send_config_pattern_once(&client)
            .await
            .context("initial ConfigPattern")?;
    }
    if cli.gpio_base_signal {
        send_set_gpio_out_once(&client)
            .await
            .context("initial SetGpioOut")?;
        eprintln!("GPIO[0]: BaseSignal (probe it to check inter-device sync)");
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    spawn_signal_listener(Arc::clone(&shutdown));

    let sender = Sender::new(&client, &geometry, cli)?;

    let output = match cli.mode {
        Mode::StopAndWait => run_stop_and_wait(&client, cli, sender, shutdown, &link_stats).await,
        Mode::Streaming => {
            run_streaming(&client, cli, sender, shutdown, max_inflight, &link_stats).await
        }
    };

    let _ = client.close().await;

    output
}

async fn run_stop_and_wait(
    client: &Client,
    cli: &Cli,
    mut sender: Sender,
    shutdown: Arc<AtomicBool>,
    link_stats: &LinkStats,
) -> Result<RunOutput> {
    let mut recorded = Recorder::new(cli);
    let mut index: u64 = 0;
    let mut stopped_on_error = None;
    let mut rt_closed = false;
    let mut progress = Progress::new(cli);

    let mem_recorder = mem::start();
    let start = Instant::now();
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        if let Some(n) = cli.count
            && index >= n
        {
            break;
        }
        if let Some(d) = cli.duration
            && start.elapsed() >= d
        {
            break;
        }

        sender.prepare(client)?;
        let t0 = Instant::now();
        let res = client
            .send_checked(sender.frames.frame(0).expect("one frame"))
            .await;
        let rtt = t0.elapsed();

        let status = match res {
            Ok(()) => SampleStatus::Ok,
            Err(ClientError::DeviceError { code, .. }) => SampleStatus::DeviceError(code),
            Err(ClientError::Timeout { .. }) => SampleStatus::Timeout,
            Err(ClientError::Link(msg)) => {
                eprintln!("link error: {msg}");
                SampleStatus::LinkError
            }
            Err(ClientError::InvalidPayload(e)) => {
                anyhow::bail!("payload rejected by the local encoder: {e}");
            }
            Err(ClientError::Encode(e)) => {
                anyhow::bail!("value rejected by the local encoder: {e}");
            }
            Err(e @ ClientError::SilencerConstraint { .. }) => {
                anyhow::bail!("rejected by the local silencer precheck: {e}");
            }
            Err(e @ ClientError::TransitionConstraint { .. }) => {
                anyhow::bail!("rejected by the local transition precheck: {e}");
            }
            Err(e @ ClientError::UnsupportedFirmware { .. }) => anyhow::bail!("{e}"),
            Err(ClientError::RtClosed) => {
                eprintln!("client RT thread closed unexpectedly");
                rt_closed = true;
                SampleStatus::LinkError
            }
        };

        recorded.push(Sample { index, rtt, status });
        progress.observe(status, start.elapsed());
        if rt_closed {
            break;
        }
        if cli.stop_on_error && status != SampleStatus::Ok {
            stopped_on_error = Some((index, status));
            break;
        }
        index += 1;
    }

    progress.finish();

    let mem = mem::profile(mem_recorder, recorded.sends);
    Ok(RunOutput {
        samples: recorded.samples,
        sends: recorded.sends,
        stopped_on_error,
        rt_closed,
        warmup: cli.warmup,
        elapsed: start.elapsed(),
        frame_bytes: TX_FRAME_BYTES,
        stale_cycles: link_stats.stale_cycles(),
        lost_cycles: link_stats.lost_cycles(),
        mem,
    })
}

async fn run_streaming(
    client: &Client,
    cli: &Cli,
    mut sender: Sender,
    shutdown: Arc<AtomicBool>,
    max_inflight: usize,
    link_stats: &LinkStats,
) -> Result<RunOutput> {
    let mut recorded = Recorder::new(cli);
    let mut pending: VecDeque<PendingFuture> = VecDeque::with_capacity(max_inflight);
    let mut sends_issued: u64 = 0;
    let mut sample_index: u64 = 0;
    let mut stopped_on_error = None;
    let mut rt_closed = false;
    let mut progress = Progress::new(cli);

    let mem_recorder = mem::start();
    let start = Instant::now();
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        let need_send = streaming_need_send(cli, sends_issued, start);

        if need_send && pending.len() < max_inflight {
            sender.prepare(client)?;
            let sent_at = Instant::now();
            let fut = match client
                .send(sender.frames.frame(0).expect("one frame"))
                .await
            {
                Ok(fut) => fut,
                Err(ClientError::RtClosed) => {
                    eprintln!("client RT thread closed unexpectedly");
                    rt_closed = true;
                    break;
                }
                Err(e) => return Err(e.into()),
            };
            pending.push_back(PendingFuture { sent_at, fut });
            sends_issued += 1;
            continue;
        }

        if pending.is_empty() {
            break;
        }

        let entry = pending.pop_front().expect("non-empty");
        let res = entry.fut.await;
        let rtt = entry.sent_at.elapsed();
        let status = match res {
            Ok(resp) => match resp.data().iter().find(|&&d| d != 0) {
                None => SampleStatus::Ok,
                Some(&code) => SampleStatus::DeviceError(code),
            },
            Err(ClientError::Timeout { .. }) => SampleStatus::Timeout,
            Err(ClientError::Link(msg)) => {
                eprintln!("link error: {msg}");
                SampleStatus::LinkError
            }
            Err(ClientError::DeviceError { code, .. }) => SampleStatus::DeviceError(code),
            Err(ClientError::InvalidPayload(e)) => {
                anyhow::bail!("payload rejected by the local encoder: {e}");
            }
            Err(ClientError::Encode(e)) => {
                anyhow::bail!("value rejected by the local encoder: {e}");
            }
            Err(e @ ClientError::SilencerConstraint { .. }) => {
                anyhow::bail!("rejected by the local silencer precheck: {e}");
            }
            Err(e @ ClientError::TransitionConstraint { .. }) => {
                anyhow::bail!("rejected by the local transition precheck: {e}");
            }
            Err(e @ ClientError::UnsupportedFirmware { .. }) => anyhow::bail!("{e}"),
            Err(ClientError::RtClosed) => {
                eprintln!("client RT thread closed unexpectedly");
                rt_closed = true;
                SampleStatus::LinkError
            }
        };
        recorded.push(Sample {
            index: sample_index,
            rtt,
            status,
        });
        progress.observe(status, start.elapsed());
        if rt_closed {
            break;
        }
        if cli.stop_on_error && status != SampleStatus::Ok {
            stopped_on_error = Some((sample_index, status));
            break;
        }
        sample_index += 1;
    }

    progress.finish();

    let mem = mem::profile(mem_recorder, recorded.sends);
    Ok(RunOutput {
        samples: recorded.samples,
        sends: recorded.sends,
        stopped_on_error,
        rt_closed,
        warmup: cli.warmup,
        elapsed: start.elapsed(),
        frame_bytes: TX_FRAME_BYTES,
        stale_cycles: link_stats.stale_cycles(),
        lost_cycles: link_stats.lost_cycles(),
        mem,
    })
}

struct PendingFuture {
    sent_at: Instant,
    fut: ResponseFuture,
}

fn streaming_need_send(cli: &Cli, sends_issued: u64, start: Instant) -> bool {
    if let Some(n) = cli.count
        && sends_issued >= n
    {
        return false;
    }
    if let Some(d) = cli.duration
        && start.elapsed() >= d
    {
        return false;
    }
    true
}

struct Counters {
    ok: u64,
    timeouts: u64,
    device_errors: u64,
    link_errors: u64,
}

impl std::fmt::Display for Counters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ok={} timeout={} dev_err={} link_err={}    ",
            self.ok, self.timeouts, self.device_errors, self.link_errors
        )
    }
}

struct Progress {
    count_total: Option<u64>,
    duration_total: Option<Duration>,
    completed: u64,
    ok: u64,
    timeouts: u64,
    link_errors: u64,
    device_errors: u64,
    last_render: Instant,
    rendered_once: bool,
}

impl Progress {
    fn new(cli: &Cli) -> Self {
        Self {
            count_total: cli.count,
            duration_total: cli.duration,
            completed: 0,
            ok: 0,
            timeouts: 0,
            link_errors: 0,
            device_errors: 0,
            last_render: Instant::now()
                .checked_sub(PROGRESS_INTERVAL)
                .unwrap_or_else(Instant::now),
            rendered_once: false,
        }
    }

    fn observe(&mut self, status: SampleStatus, elapsed: Duration) {
        self.completed += 1;
        match status {
            SampleStatus::Ok => self.ok += 1,
            SampleStatus::Timeout => self.timeouts += 1,
            SampleStatus::LinkError => self.link_errors += 1,
            SampleStatus::DeviceError(_) => self.device_errors += 1,
        }
        let now = Instant::now();
        if now.duration_since(self.last_render) >= PROGRESS_INTERVAL {
            self.render(elapsed);
            self.last_render = now;
        }
    }

    fn render(&mut self, elapsed: Duration) {
        let tail = Counters {
            ok: self.ok,
            timeouts: self.timeouts,
            device_errors: self.device_errors,
            link_errors: self.link_errors,
        };
        if let Some(total) = self.count_total {
            eprint!("\r[{:>8}/{total}] {tail}", self.completed);
        } else if let Some(total) = self.duration_total {
            eprint!(
                "\r[{:>6.1}/{:.1}s] {tail}",
                elapsed.as_secs_f64(),
                total.as_secs_f64()
            );
        } else {
            eprint!(
                "\r[{:>8} ({:.1}s)] {tail}",
                self.completed,
                elapsed.as_secs_f64()
            );
        }
        let _ = std::io::Write::flush(&mut std::io::stderr());
        self.rendered_once = true;
    }

    fn finish(&mut self) {
        if self.rendered_once {
            eprintln!();
        }
    }
}

fn spawn_signal_listener(flag: Arc<AtomicBool>) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            flag.store(true, Ordering::Relaxed);
            eprintln!("\nCtrl+C received — stopping after the current sample...");
        }
    });
}

fn estimate_capacity(cli: &Cli) -> usize {
    if let Some(n) = cli.count {
        return usize::try_from(n).unwrap_or(usize::MAX);
    }
    if let Some(d) = cli.duration
        && let Some(cycles) = d.as_micros().checked_div(cli.sync0_period.as_micros())
    {
        return cycles as usize;
    }
    0
}
