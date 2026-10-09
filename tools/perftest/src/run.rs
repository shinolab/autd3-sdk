use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs::commands::{
    ActivatePatternBank, ConfigPattern, GpioOut, Nop, Pattern, SetGpioOut, WriteModulationBuffer,
    WritePatternBuffer,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::params::MOD_BUFFER_SAMPLES;
use autd3_rs::protocol::FrameHeader;
use autd3_rs::value::{
    Intensity, LoopBehavior, ModulationBank, PatternBank, Phase, SamplingConfig, TransitionMode,
};
use autd3_rs::{
    BusStats, Client, ClientConfig, Error as ClientError, Frame, Frames, Interface, ResponseFuture,
    StateChecker, Telemetry, TelemetryCounters, TransportOption,
};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use crate::cli::{Cli, Command, Mode};
use crate::mem::{self, MemProfile};
use crate::stats::{DriverAckLatency, Sample, SampleStatus};

const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
const STATE_CHECK_INTERVAL: Duration = Duration::from_millis(100);

pub struct RunOutput {
    pub samples: Vec<Sample>,
    pub sends: u64,
    pub stopped_on_error: Option<(u64, SampleStatus)>,
    pub driver_closed: bool,
    pub elapsed: Duration,
    pub frame_bytes: usize,
    pub missed_replies: u64,
    pub driver_ack: Option<DriverAckLatency>,
    pub mem: Option<MemProfile>,
}

struct Sender {
    command: Command,
    frames: Frames,
    phases: Vec<Vec<Phase>>,
    intensities: Vec<Vec<Intensity>>,
    modulation: Vec<u8>,
    tick: u8,
    next_frame: usize,
}

impl Sender {
    fn new(geometry: &Geometry, cli: &Cli) -> Result<Self> {
        let mut sender = Self {
            command: cli.command,
            frames: Frames::default(),
            phases: if cli.command.is_pattern() {
                geometry.phase_buffer()
            } else {
                Vec::new()
            },
            intensities: if cli.command.is_pattern() {
                geometry
                    .iter()
                    .map(|d| vec![Intensity::MIN; d.num_transducers()])
                    .collect()
            } else {
                Vec::new()
            },
            modulation: if cli.command.is_bulk() {
                vec![0; MOD_BUFFER_SAMPLES]
            } else {
                Vec::new()
            },
            tick: 0,
            next_frame: 0,
        };
        if cli.command == Command::Nop {
            sender
                .frames
                .encode_into(geometry, Nop)
                .context("building Nop frame")?;
        }
        Ok(sender)
    }

    fn prepare(&mut self, client: &Client) -> Result<()> {
        if self.command == Command::Nop {
            return Ok(());
        }
        if self.command.is_bulk() {
            let mut value = self.tick;
            for sample in &mut self.modulation {
                *sample = value;
                value = value.wrapping_add(1);
            }
            self.tick = self.tick.wrapping_add(1);
            self.frames
                .encode_into(
                    client.geometry(),
                    WriteModulationBuffer {
                        bank: ModulationBank::B1,
                        offset: 0,
                        data: &self.modulation,
                    },
                )
                .context("encoding modulation write")?;
            return Ok(());
        }
        fill_phases(&mut self.phases, self.tick);
        self.tick = self.tick.wrapping_add(1);

        let encoded = if self.command == Command::Pattern {
            self.frames.encode_into(
                client.geometry(),
                Pattern::with_bank(PatternBank::B0, &self.phases, &self.intensities),
            )
        } else {
            self.frames.encode_into(
                client.geometry(),
                WritePatternBuffer::new(PatternBank::B0, 0, &self.phases, &self.intensities),
            )
        };
        encoded.context("encoding pattern write")?;
        Ok(())
    }

    fn next_frame(&mut self, client: &Client) -> Result<Frame<'_>> {
        if self.next_frame >= self.frames.len() {
            self.prepare(client)?;
            self.next_frame = 0;
        }
        let frame = self
            .frames
            .frame(self.next_frame)
            .context("the command built no frame")?;
        self.next_frame += 1;
        Ok(frame)
    }

    fn frame_bytes(&self) -> usize {
        self.frames
            .frame(0)
            .map_or(size_of::<FrameHeader>(), |frame| {
                frame
                    .datagrams()
                    .iter()
                    .map(|d| size_of::<FrameHeader>() + d.payload_len)
                    .max()
                    .unwrap_or(size_of::<FrameHeader>())
            })
    }
}

fn fill_phases(phases: &mut [Vec<Phase>], tick: u8) {
    for device in phases {
        let mut phase = tick;
        for p in device.iter_mut() {
            *p = Phase(phase);
            phase = phase.wrapping_add(1);
        }
    }
}

async fn send_config_pattern_once(client: &Client) -> Result<()> {
    client
        .send((
            ConfigPattern {
                bank: PatternBank::B0,
                config: SamplingConfig::FREQ_4K,
                size: 1,
                loop_behavior: LoopBehavior::Infinite,
            },
            ActivatePatternBank {
                bank: PatternBank::B0,
                transition_mode: TransitionMode::Immediate,
            },
        ))
        .await?;
    Ok(())
}

async fn send_set_gpio_out_once(client: &Client, output: GpioOut) -> Result<()> {
    client
        .send(SetGpioOut {
            outputs: [output, GpioOut::Off, GpioOut::Off, GpioOut::Off],
        })
        .await?;
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

fn spawn_state_check(checker: StateChecker, interval: Duration) -> StateCheckGuard {
    let stop = Arc::new(AtomicBool::new(false));
    let join = tokio::spawn({
        let stop = Arc::clone(&stop);
        async move {
            while !stop.load(Ordering::Relaxed) {
                if checker.check().is_err() {
                    break;
                }
                tokio::time::sleep(interval).await;
            }
        }
    });
    StateCheckGuard { stop, join }
}

pub async fn run(cli: &Cli) -> Result<RunOutput> {
    let emulator = cli
        .emulator
        .then(|| UdpEmulator::spawn(cli.devices.get()))
        .transpose()
        .context("starting the UDP device emulator")?;
    let mut option = match &emulator {
        Some(emulator) => TransportOption {
            iface: emulator.interface(),
            reply_timeout: Duration::from_millis(50),
            response_timeout: Duration::from_millis(50),
            enumeration_timeout: Duration::from_secs(1),
            sync_timeout: Duration::from_secs(5),
            ..Default::default()
        },
        None => TransportOption {
            iface: if cli.simulator {
                Interface::Simulator
            } else {
                cli.interface.clone().into()
            },
            ..Default::default()
        },
    };
    option.heartbeat = Some(cli.heartbeat);
    if let Some(reply_timeout) = cli.reply_timeout {
        option.reply_timeout = reply_timeout;
    }
    option.send_rate_limit = cli.send_rate_limit;
    if let Some(bytes) = cli.send_buffer {
        option.send_buffer = std::num::NonZeroUsize::new(bytes);
    }
    let out = Box::pin(run_with_option(option, cli)).await;
    drop(emulator);
    out
}

async fn run_with_option(option: TransportOption, cli: &Cli) -> Result<RunOutput> {
    let num_devices = cli.devices.get();
    eprintln!("devices: {num_devices}");

    let max_inflight = match cli.mode {
        Mode::StopAndWait if !cli.command.is_bulk() => 1,
        _ => cli.max_inflight.max(1),
    };
    let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()).collect());
    let client = Box::pin(Client::open(
        &geometry,
        &option,
        ClientConfig {
            ack_timeout: cli.ack_timeout,
            max_inflight: NonZeroUsize::new(max_inflight).unwrap(),
            max_resync_rounds: cli.max_resync_rounds,
            ..Default::default()
        },
    ))
    .await
    .context("opening the devices")?;
    let checker = client.state_checker();
    let guard = spawn_state_check(checker, STATE_CHECK_INTERVAL);

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
            .context("initial ConfigPattern + ActivatePatternBank")?;
    }
    if cli.gpio_base_signal {
        send_set_gpio_out_once(&client, GpioOut::BaseSignal)
            .await
            .context("initial SetGpioOut")?;
        eprintln!("GPIO[0]: BaseSignal (probe it to check inter-device sync)");
    }
    if cli.gpio_sync {
        send_set_gpio_out_once(&client, GpioOut::Sync)
            .await
            .context("initial SetGpioOut")?;
        eprintln!("GPIO[0]: Sync (probe it to check the sync pulse of every device)");
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    spawn_signal_listener(Arc::clone(&shutdown));

    let telemetry_before = if cli.telemetry {
        Some(client.read_telemetry().await.context("reading telemetry")?)
    } else {
        None
    };
    if let Some(hold) = cli.hold {
        eprintln!("holding the connection idle for {hold:?}");
        tokio::time::sleep(hold).await;
    }

    let sender = Sender::new(&geometry, cli)?;
    let bus_stats = client.bus_stats();

    let output = match cli.mode {
        Mode::StopAndWait => run_stop_and_wait(&client, cli, sender, shutdown, &bus_stats).await,
        Mode::Streaming => {
            run_streaming(&client, cli, sender, shutdown, max_inflight, &bus_stats).await
        }
    };

    if let Some(before) = telemetry_before {
        let after = client.read_telemetry().await.context("reading telemetry")?;
        print_telemetry(&before, &after);
    }

    if let Err(e) = client.close().await {
        eprintln!("the client closed with an error: {e}");
    }
    guard.stop().await;

    output
}

fn print_telemetry(before: &[TelemetryCounters], after: &[TelemetryCounters]) {
    for counter in Telemetry::ALL {
        let at_start: Vec<u32> = before.iter().map(|c| c.get(*counter)).collect();
        let deltas: Vec<u32> = before
            .iter()
            .zip(after)
            .map(|(b, a)| a.get(*counter).wrapping_sub(b.get(*counter)))
            .collect();
        eprintln!("telemetry {counter:?}: at start {at_start:?}, delta {deltas:?}");
    }
}

async fn send_all(client: &Client, frames: &Frames, window: usize) -> Result<(), ClientError> {
    let mut pending: VecDeque<ResponseFuture> = VecDeque::with_capacity(window);
    for frame in frames {
        if pending.len() >= window {
            let fut = pending.pop_front().expect("non-empty");
            fut.await?.check()?;
        }
        pending.push_back(client.send_frame(frame).await?);
    }
    for fut in pending {
        fut.await?.check()?;
    }
    Ok(())
}

fn budget_left(cli: &Cli, issued: u64, start: Instant) -> bool {
    if let Some(n) = cli.count
        && issued >= n
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

#[derive(Clone, Copy)]
enum Outcome {
    Sample(SampleStatus),
    DriverClosed,
}

fn classify(res: Result<(), ClientError>) -> Result<Outcome> {
    Ok(Outcome::Sample(match res {
        Ok(()) => SampleStatus::Ok,
        Err(ClientError::DeviceError { code, .. }) => SampleStatus::DeviceError(code),
        Err(ClientError::Timeout { .. }) => SampleStatus::Timeout,
        Err(ClientError::Network(cause)) => {
            eprintln!("network error: {cause}");
            SampleStatus::NetworkError
        }
        Err(ClientError::InvalidPayload(e)) => {
            anyhow::bail!("payload rejected by the local encoder: {e}");
        }
        Err(ClientError::Encode(e)) => {
            anyhow::bail!("value rejected by the local encoder: {e}");
        }
        Err(ClientError::Closed) => {
            eprintln!("the client closed unexpectedly");
            return Ok(Outcome::DriverClosed);
        }
        Err(e) => anyhow::bail!("{e}"),
    }))
}

#[derive(Clone, Copy)]
struct AckSnapshot {
    frames: u64,
    total_ns: u128,
}

impl AckSnapshot {
    fn take(bus_stats: &BusStats) -> Self {
        let frames = bus_stats.acked_frames();
        Self {
            frames,
            total_ns: u128::from(bus_stats.mean_ack_latency_ns()) * u128::from(frames),
        }
    }

    fn latency_since(self, before: Self, worst_ns: u64) -> Option<DriverAckLatency> {
        let frames = self.frames.saturating_sub(before.frames);
        if frames == 0 {
            return None;
        }
        let mean_ns = self.total_ns.saturating_sub(before.total_ns) / u128::from(frames);
        Some(DriverAckLatency {
            frames,
            mean: Duration::from_nanos(u64::try_from(mean_ns).unwrap_or(u64::MAX)),
            worst: Duration::from_nanos(worst_ns),
        })
    }
}

struct Measurement {
    recorded: Recorder,
    progress: Progress,
    stopped_on_error: Option<(u64, SampleStatus)>,
    driver_closed: bool,
    stop_on_error: bool,
    mem_recorder: mem::Recorder,
    start: Instant,
    bus_stats: BusStats,
    warmup: u64,
    ack_before: Option<AckSnapshot>,
}

impl Measurement {
    fn start(cli: &Cli, bus_stats: &BusStats) -> Self {
        let recorded = Recorder::new(cli);
        let progress = Progress::new(cli);
        let mem_recorder = mem::start();
        Self {
            recorded,
            progress,
            stopped_on_error: None,
            driver_closed: false,
            stop_on_error: cli.stop_on_error,
            mem_recorder,
            start: Instant::now(),
            bus_stats: bus_stats.clone(),
            warmup: cli.warmup,
            ack_before: (cli.warmup == 0).then(|| AckSnapshot::take(bus_stats)),
        }
    }

    fn record(&mut self, index: u64, rtt: Duration, outcome: Outcome) -> ControlFlow<()> {
        let status = match outcome {
            Outcome::Sample(status) => status,
            Outcome::DriverClosed => {
                self.driver_closed = true;
                SampleStatus::NetworkError
            }
        };
        self.recorded.push(Sample { index, rtt, status });
        if self.ack_before.is_none() && self.recorded.sends == self.warmup {
            self.ack_before = Some(AckSnapshot::take(&self.bus_stats));
        }
        self.progress.observe(status, self.start.elapsed());
        if self.driver_closed {
            return ControlFlow::Break(());
        }
        if self.stop_on_error && status != SampleStatus::Ok {
            self.stopped_on_error = Some((index, status));
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    fn finish(mut self, sender: &Sender) -> RunOutput {
        self.progress.finish();
        let bus_stats = &self.bus_stats;
        let driver_ack = self.ack_before.and_then(|before| {
            AckSnapshot::take(bus_stats).latency_since(before, bus_stats.worst_ack_latency_ns())
        });
        let mem = mem::profile(self.mem_recorder, self.recorded.sends);
        RunOutput {
            samples: self.recorded.samples,
            sends: self.recorded.sends,
            stopped_on_error: self.stopped_on_error,
            driver_closed: self.driver_closed,
            elapsed: self.start.elapsed(),
            frame_bytes: sender.frame_bytes(),
            missed_replies: bus_stats.missed_replies(),
            driver_ack,
            mem,
        }
    }
}

async fn run_stop_and_wait(
    client: &Client,
    cli: &Cli,
    mut sender: Sender,
    shutdown: Arc<AtomicBool>,
    bus_stats: &BusStats,
) -> Result<RunOutput> {
    let window = if cli.command.is_bulk() {
        cli.max_inflight.max(1)
    } else {
        1
    };
    let mut index: u64 = 0;
    let mut measurement = Measurement::start(cli, bus_stats);
    while !shutdown.load(Ordering::Relaxed) && budget_left(cli, index, measurement.start) {
        sender.prepare(client)?;
        let t0 = Instant::now();
        let res = send_all(client, &sender.frames, window).await;
        let rtt = t0.elapsed();

        if measurement.record(index, rtt, classify(res)?).is_break() {
            break;
        }
        index += 1;
    }
    Ok(measurement.finish(&sender))
}

async fn run_streaming(
    client: &Client,
    cli: &Cli,
    mut sender: Sender,
    shutdown: Arc<AtomicBool>,
    max_inflight: usize,
    bus_stats: &BusStats,
) -> Result<RunOutput> {
    let mut pending: VecDeque<PendingFuture> = VecDeque::with_capacity(max_inflight);
    let mut sends_issued: u64 = 0;
    let mut sample_index: u64 = 0;
    let mut measurement = Measurement::start(cli, bus_stats);
    while !shutdown.load(Ordering::Relaxed) {
        let need_send = budget_left(cli, sends_issued, measurement.start);

        if need_send && pending.len() < max_inflight {
            let frame = sender.next_frame(client)?;
            let sent_at = Instant::now();
            let fut = match client.send_frame(frame).await {
                Ok(fut) => fut,
                Err(ClientError::Closed) => {
                    eprintln!("the client closed unexpectedly");
                    measurement.driver_closed = true;
                    break;
                }
                Err(e) => return Err(e.into()),
            };
            pending.push_back(PendingFuture { sent_at, fut });
            sends_issued += 1;
            continue;
        }

        let Some(entry) = pending.pop_front() else {
            break;
        };
        let res = entry.fut.await;
        let rtt = entry.sent_at.elapsed();
        let outcome = classify(res.and_then(|resp| resp.check()))?;
        if measurement.record(sample_index, rtt, outcome).is_break() {
            break;
        }
        sample_index += 1;
    }
    Ok(measurement.finish(&sender))
}

struct PendingFuture {
    sent_at: Instant,
    fut: ResponseFuture,
}

#[derive(Default)]
struct Counters {
    ok: u64,
    timeouts: u64,
    device_errors: u64,
    network_errors: u64,
}

impl std::fmt::Display for Counters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ok={} timeout={} dev_err={} net_err={}    ",
            self.ok, self.timeouts, self.device_errors, self.network_errors
        )
    }
}

struct Progress {
    count_total: Option<u64>,
    duration_total: Option<Duration>,
    completed: u64,
    counters: Counters,
    last_render: Instant,
    rendered_once: bool,
}

impl Progress {
    fn new(cli: &Cli) -> Self {
        Self {
            count_total: cli.count,
            duration_total: cli.duration,
            completed: 0,
            counters: Counters::default(),
            last_render: Instant::now()
                .checked_sub(PROGRESS_INTERVAL)
                .unwrap_or_else(Instant::now),
            rendered_once: false,
        }
    }

    fn observe(&mut self, status: SampleStatus, elapsed: Duration) {
        self.completed += 1;
        match status {
            SampleStatus::Ok => self.counters.ok += 1,
            SampleStatus::Timeout => self.counters.timeouts += 1,
            SampleStatus::NetworkError => self.counters.network_errors += 1,
            SampleStatus::DeviceError(_) => self.counters.device_errors += 1,
        }
        let now = Instant::now();
        if now.duration_since(self.last_render) >= PROGRESS_INTERVAL {
            self.render(elapsed);
            self.last_render = now;
        }
    }

    fn render(&mut self, elapsed: Duration) {
        let tail = &self.counters;
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
    if let Some(d) = cli.duration {
        return usize::try_from(d.as_millis()).unwrap_or(usize::MAX);
    }
    0
}
