use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs::commands::{
    ConfigPattern, GpioOut, Nop, Pattern, SetGpioOut, WriteModulationBuffer, WritePatternBuffer,
};
use autd3_rs::driver::Poll;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::params::MOD_BUFFER_SAMPLES;
use autd3_rs::protocol::FRAME_HEADER_BYTES;
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs::value::{
    Intensity, LoopBehavior, ModulationBank, PatternBank, Phase, SamplingConfig,
};
use autd3_rs::{
    BusStats, Client, ClientConfig, Driver, Error as ClientError, Frames, ResponseFuture,
    StateChecker, Telemetry, TelemetryCounters, TransportOption,
};

use crate::cli::{Cli, Command, Mode};
use crate::mem::{self, MemProfile};
use crate::stats::{Sample, SampleStatus};
use crate::tune::{self, ThreadTuning};

const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
const STATE_CHECK_INTERVAL: Duration = Duration::from_millis(100);

pub struct RunOutput {
    pub samples: Vec<Sample>,
    pub sends: u64,
    pub stopped_on_error: Option<(u64, SampleStatus)>,
    pub driver_closed: bool,
    pub warmup: u64,
    pub elapsed: Duration,
    pub frame_bytes: usize,
    pub retransmissions: u64,
    pub missed_replies: u64,
    pub mem: Option<MemProfile>,
}

struct Sender {
    command: Command,
    frames: Frames,
    phases: Vec<Vec<Phase>>,
    intensities: Vec<Vec<Intensity>>,
    modulation: Vec<u8>,
    tick: u8,
}

impl Sender {
    fn new(client: &Client, geometry: &Geometry, cli: &Cli) -> Result<Self> {
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
        if self.command.is_bulk() {
            let mut value = self.tick;
            for sample in &mut self.modulation {
                *sample = value;
                value = value.wrapping_add(1);
            }
            self.tick = self.tick.wrapping_add(1);
            let mut builder = client.datagram_builder();
            builder.push(WriteModulationBuffer {
                bank: ModulationBank::B1,
                offset: 0,
                data: &self.modulation,
            });
            builder
                .build_into(&mut self.frames)
                .context("encoding modulation write")?;
            return Ok(());
        }
        fill_phases(&mut self.phases, self.tick);
        self.tick = self.tick.wrapping_add(1);

        let mut builder = client.datagram_builder();
        if self.command == Command::Pattern {
            builder.push(Pattern::with_bank(
                PatternBank::B0,
                &self.phases,
                &self.intensities,
            ));
        } else {
            builder.push(WritePatternBuffer::new(
                PatternBank::B0,
                0,
                &self.phases,
                &self.intensities,
            ));
        }
        builder
            .build_into(&mut self.frames)
            .context("encoding pattern write")?;
        Ok(())
    }
}

impl Sender {
    fn frame_bytes(&self) -> usize {
        self.frames.frame(0).map_or(FRAME_HEADER_BYTES, |frame| {
            frame
                .datagrams()
                .iter()
                .map(|d| {
                    FRAME_HEADER_BYTES
                        + d.payload.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1)
                })
                .max()
                .unwrap_or(FRAME_HEADER_BYTES)
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

fn spawn_state_check(mut checker: StateChecker, interval: Duration) -> StateCheckGuard {
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
        .then(|| UdpEmulator::spawn(cli.devices))
        .transpose()
        .context("starting the UDP device emulator")?;
    let mut option = match &emulator {
        Some(emulator) => emulator.option(),
        None => TransportOption {
            iface: cli.interface.clone().into(),
            group: cli.group,
            ..Default::default()
        },
    };
    option.heartbeat = cli.heartbeat;
    if let Some(reply_timeout) = cli.reply_timeout {
        option.reply_timeout = reply_timeout;
    }
    let out = Box::pin(run_with_option(option, cli.devices, cli)).await;
    drop(emulator);
    out
}

async fn run_with_option(
    option: TransportOption,
    num_devices: usize,
    cli: &Cli,
) -> Result<RunOutput> {
    eprintln!("devices: {num_devices}");

    let max_inflight = match cli.mode {
        Mode::StopAndWait if !cli.command.is_bulk() => 1,
        _ => cli.max_inflight.max(1),
    };
    let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()).collect());
    let (mut driver, connector) =
        Driver::open(&option, num_devices).context("opening the devices")?;
    let checker = driver.state_checker();
    let tuning = ThreadTuning::from(cli);
    let poll_sleep = cli.poll_sleep;
    let driver = std::thread::Builder::new()
        .name("autd3-driver".to_owned())
        .spawn(move || {
            tune::apply(tuning);
            match poll_sleep {
                Some(nap) => {
                    while let Poll::Next(_) = driver.poll() {
                        std::thread::sleep(nap);
                    }
                    driver.close()
                }
                None => driver.run(),
            }
        })
        .context("spawning the driver thread")?;
    let client = Box::pin(Client::open(
        &geometry,
        connector,
        ClientConfig {
            ack_timeout: cli.ack_timeout,
            max_inflight: NonZeroUsize::new(max_inflight).unwrap(),
            max_resync_rounds: cli.max_resync_rounds,
            low_latency: cli.low_latency,
            validate_state: false,
            ..Default::default()
        },
    ))
    .await
    .context("client handshake")?;
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

    let telemetry_before = if cli.telemetry {
        Some(client.read_telemetry().await.context("reading telemetry")?)
    } else {
        None
    };
    if let Some(hold) = cli.hold {
        eprintln!("holding the connection idle for {hold:?}");
        tokio::time::sleep(hold).await;
    }

    let sender = Sender::new(&client, &geometry, cli)?;
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

    let _ = client.close().await;
    guard.stop().await;
    match driver.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("the driver closed with an error: {e}"),
        Err(_) => eprintln!("the driver thread panicked"),
    }

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
        pending.push_back(client.send(frame).await?);
    }
    for fut in pending {
        fut.await?.check()?;
    }
    Ok(())
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
    let mut recorded = Recorder::new(cli);
    let mut index: u64 = 0;
    let mut stopped_on_error = None;
    let mut driver_closed = false;
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
        let res = send_all(client, &sender.frames, window).await;
        let rtt = t0.elapsed();

        let status = match res {
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
            Err(e @ ClientError::SilencerConstraint { .. }) => {
                anyhow::bail!("rejected by the local silencer precheck: {e}");
            }
            Err(e @ ClientError::TransitionConstraint { .. }) => {
                anyhow::bail!("rejected by the local transition precheck: {e}");
            }
            Err(ClientError::DriverClosed) => {
                eprintln!("the driver closed unexpectedly");
                driver_closed = true;
                SampleStatus::NetworkError
            }
            Err(e) => anyhow::bail!("{e}"),
        };

        recorded.push(Sample { index, rtt, status });
        progress.observe(status, start.elapsed());
        if driver_closed {
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
        driver_closed,
        warmup: cli.warmup,
        elapsed: start.elapsed(),
        frame_bytes: sender.frame_bytes(),
        retransmissions: bus_stats.retransmissions(),
        missed_replies: bus_stats.missed_replies(),
        mem,
    })
}

async fn run_streaming(
    client: &Client,
    cli: &Cli,
    mut sender: Sender,
    shutdown: Arc<AtomicBool>,
    max_inflight: usize,
    bus_stats: &BusStats,
) -> Result<RunOutput> {
    let mut recorded = Recorder::new(cli);
    let mut pending: VecDeque<PendingFuture> = VecDeque::with_capacity(max_inflight);
    let mut sends_issued: u64 = 0;
    let mut sample_index: u64 = 0;
    let mut stopped_on_error = None;
    let mut driver_closed = false;
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
                Err(ClientError::DriverClosed) => {
                    eprintln!("the driver closed unexpectedly");
                    driver_closed = true;
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
            Ok(resp) => match resp.status().iter().find(|&&d| d != 0) {
                None => SampleStatus::Ok,
                Some(&code) => SampleStatus::DeviceError(code),
            },
            Err(ClientError::Timeout { .. }) => SampleStatus::Timeout,
            Err(ClientError::Network(cause)) => {
                eprintln!("network error: {cause}");
                SampleStatus::NetworkError
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
            Err(ClientError::DriverClosed) => {
                eprintln!("the driver closed unexpectedly");
                driver_closed = true;
                SampleStatus::NetworkError
            }
            Err(e) => anyhow::bail!("{e}"),
        };
        recorded.push(Sample {
            index: sample_index,
            rtt,
            status,
        });
        progress.observe(status, start.elapsed());
        if driver_closed {
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
        driver_closed,
        warmup: cli.warmup,
        elapsed: start.elapsed(),
        frame_bytes: sender.frame_bytes(),
        retransmissions: bus_stats.retransmissions(),
        missed_replies: bus_stats.missed_replies(),
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
    ok: u64,
    timeouts: u64,
    network_errors: u64,
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
            network_errors: 0,
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
            SampleStatus::NetworkError => self.network_errors += 1,
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
            network_errors: self.network_errors,
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
    if let Some(d) = cli.duration {
        return usize::try_from(d.as_millis()).unwrap_or(usize::MAX);
    }
    0
}
