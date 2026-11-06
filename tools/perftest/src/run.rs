use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs::commands::{
    ActivatePatternBank, Clear, ConfigPattern, GpioOut, Nop, Pattern, SetGpioOut, Synchronize,
    WritePatternBuffer,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{
    Intensity, LoopBehavior, PatternBank, Phase, SamplingConfig, TransitionMode,
};
use autd3_rs::{
    Controller, DeviceReply, Error as ClientError, ExecAt, Frame, Frames, Interface, Telemetry,
    TransportOption,
};
use tokio::time::error::Elapsed;

use crate::cli::{Cli, Command, Mode};
use crate::report::{LiveBlock, interval_lines, telemetry_lines};
use crate::stats::{
    AckLatency, Counters, SampleStatus, Summary, Traffic, latency, record_rtt, rtt_histogram,
};

pub(crate) struct RunOutput {
    pub summary: Summary,
    pub stopped_on_error: Option<(u64, SampleStatus)>,
    pub device_lost: bool,
    pub telemetry_start: Vec<Telemetry>,
    pub telemetry_end: Vec<Telemetry>,
}

struct Sender {
    command: Command,
    frames: Frames,
    phases: Vec<Vec<Phase>>,
    intensities: Vec<Vec<Intensity>>,
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

    fn prepare(&mut self, client: &Controller) -> Result<()> {
        if self.command == Command::Nop {
            return Ok(());
        }
        for device in &mut self.phases {
            let mut phase = self.tick;
            for p in device.iter_mut() {
                *p = Phase(phase);
                phase = phase.wrapping_add(1);
            }
        }
        self.tick = self.tick.wrapping_add(1);

        let encoded = if self.command == Command::Pattern {
            self.frames.encode_into(
                client.geometry(),
                Pattern::with_bank(PatternBank::B0, &self.phases, &self.intensities),
            )
        } else {
            self.frames.encode_into(
                client.geometry(),
                WritePatternBuffer::new(
                    PatternBank::B0,
                    0,
                    std::slice::from_ref(&self.phases),
                    &self.intensities,
                ),
            )
        };
        encoded.context("encoding pattern write")?;
        Ok(())
    }

    fn next_frame(&mut self, client: &Controller) -> Result<Frame<'_>> {
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
}

async fn run_stop_and_wait(
    client: &Controller,
    exec: ExecAt,
    ack_timeout: Duration,
    mut sender: Sender,
    shutdown: Arc<AtomicBool>,
    mut measurement: Measurement,
) -> Result<RunOutput> {
    let mut index: u64 = 0;
    while !shutdown.load(Ordering::Relaxed) {
        sender.prepare(client)?;
        let t0 = Instant::now();
        let res = async {
            for frame in &sender.frames {
                let sent_at = Instant::now();
                let msg_id = client.send_frame(frame, exec).await?;
                measurement.traffic.sent(&frame);
                let response = tokio::time::timeout(ack_timeout, client.collect(msg_id)).await??;
                measurement.ack.record(sent_at.elapsed());
                response.check()?;
                measurement.traffic.replied();
            }
            Ok::<(), Failure>(())
        }
        .await;
        let rtt = t0.elapsed();

        if measurement.record(index, rtt, classify(res)?).is_break() {
            break;
        }
        measurement.report(client).await;
        index += 1;
    }
    Ok(measurement.finish())
}

async fn run_streaming(
    client: &Controller,
    exec: ExecAt,
    ack_timeout: Duration,
    mut sender: Sender,
    shutdown: Arc<AtomicBool>,
    max_inflight: usize,
    mut measurement: Measurement,
) -> Result<RunOutput> {
    let mut pending: VecDeque<InFlight> = VecDeque::with_capacity(max_inflight);
    let mut sample_index: u64 = 0;
    while !shutdown.load(Ordering::Relaxed) {
        let report_due = measurement.live.is_due();
        if !report_due && pending.len() < max_inflight {
            let frame = sender.next_frame(client)?;
            let sent_at = Instant::now();
            let msg_id = client.send_frame(frame, exec).await?;
            measurement.traffic.sent(&frame);
            pending.push_back(InFlight::new(msg_id, sent_at, client.num_devices()));
            continue;
        }
        if pending.is_empty() {
            measurement.report(client).await;
            continue;
        }

        let waited = tokio::time::timeout(ack_timeout, async {
            while !pending[0].is_complete() {
                let reply = client.recv().await?;
                if let Some(entry) = pending.iter_mut().find(|e| e.msg_id == reply.msg_id) {
                    entry.record(&reply);
                }
            }
            Ok::<(), ClientError>(())
        })
        .await;
        let Some(head) = pending.pop_front() else {
            break;
        };
        let rtt = head.sent_at.elapsed();
        let res = waited.map_err(Failure::from).and_then(|received| {
            received?;
            measurement.ack.record(rtt);
            head.check()
        });
        let outcome = classify(res)?;
        if matches!(outcome, Outcome::Sample(SampleStatus::Ok)) {
            measurement.traffic.replied();
        }
        if measurement.record(sample_index, rtt, outcome).is_break() {
            break;
        }
        sample_index += 1;
    }
    Ok(measurement.finish())
}

struct InFlight {
    msg_id: u16,
    sent_at: Instant,
    replied: Vec<bool>,
    first_error: Option<(usize, u8)>,
}

impl InFlight {
    fn new(msg_id: u16, sent_at: Instant, num_devices: usize) -> Self {
        Self {
            msg_id,
            sent_at,
            replied: vec![false; num_devices],
            first_error: None,
        }
    }

    fn record(&mut self, reply: &DeviceReply) {
        if !std::mem::replace(&mut self.replied[reply.device], true) && reply.status != 0 {
            self.first_error.get_or_insert((reply.device, reply.status));
        }
    }

    fn is_complete(&self) -> bool {
        self.replied.iter().all(|&replied| replied)
    }

    fn check(&self) -> Result<(), Failure> {
        match self.first_error {
            None => Ok(()),
            Some((device, code)) => Err(ClientError::DeviceError { device, code }.into()),
        }
    }
}

enum Failure {
    Client(ClientError),
    Timeout,
}

impl From<ClientError> for Failure {
    fn from(e: ClientError) -> Self {
        Self::Client(e)
    }
}

impl From<Elapsed> for Failure {
    fn from(_: Elapsed) -> Self {
        Self::Timeout
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn run(cli: &Cli) -> Result<RunOutput> {
    let num_devices = cli.devices.get();
    eprintln!("devices: {num_devices}");

    let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()));
    let option = TransportOption {
        iface: cli
            .interface
            .clone()
            .map_or(Interface::Auto, Interface::Name),
        send_buffer_bytes: match cli.send_buffer {
            Some(bytes) => std::num::NonZeroUsize::new(bytes),
            None => TransportOption::default().send_buffer_bytes,
        },
        ..Default::default()
    };
    let client = Controller::open(&geometry, &option)
        .await
        .context("opening the devices")?;
    client.send(Clear).await.context("initial Clear")?;
    client
        .send(Synchronize)
        .await
        .context("initial Synchronize")?;

    let fw = client
        .read_firmware_version()
        .await
        .context("reading firmware version")?;
    for (i, fw) in fw.iter().enumerate() {
        eprintln!("device[{i}] firmware version: {fw}");
    }

    if cli.command.is_pattern() {
        client
            .send(ConfigPattern {
                bank: PatternBank::B0,
                config: SamplingConfig::FREQ_4K,
                size: 1,
                loop_behavior: LoopBehavior::Infinite,
            })
            .await
            .context("initial ConfigPattern")?;
        client
            .send(ActivatePatternBank {
                bank: PatternBank::B0,
                transition_mode: TransitionMode::Immediate,
            })
            .await
            .context("initial ActivatePatternBank")?;
    }
    client
        .send(SetGpioOut {
            outputs: [
                GpioOut::BaseSignal,
                GpioOut::Off,
                GpioOut::Off,
                GpioOut::Off,
            ],
        })
        .await
        .context("initial SetGpioOut")?;
    eprintln!("GPIO[0]: BaseSignal (probe it to check inter-device sync)");

    let shutdown = Arc::new(AtomicBool::new(false));
    spawn_signal_listener(Arc::clone(&shutdown));

    let telemetry_start = tokio::time::timeout(Duration::from_secs(1), read_telemetry(&client))
        .await
        .context("reading telemetry timed out")?
        .context("reading telemetry")?;
    let mut heartbeats = 0;
    if let Some(hold) = cli.hold {
        eprintln!("holding the connection idle for {hold:?}");
        let until = tokio::time::Instant::now() + hold;
        let mut heartbeat = tokio::time::interval(cli.heartbeat);
        while tokio::time::Instant::now() < until {
            tokio::select! {
                _ = heartbeat.tick() => {
                    client.send_heartbeat().await.context("sending a heartbeat")?;
                    heartbeats += 1;
                }
                () = tokio::time::sleep_until(until) => {}
            }
        }
    }

    client.discard_unread_replies()?;
    let sender = Sender::new(&geometry, cli)?;

    let measurement = Measurement::start(cli, heartbeats, telemetry_start);
    let exec = cli.exec_lead.map_or(ExecAt::Immediate, ExecAt::Lead);
    let mut output = match cli.mode {
        Mode::StopAndWait => {
            run_stop_and_wait(
                &client,
                exec,
                cli.ack_timeout,
                sender,
                shutdown,
                measurement,
            )
            .await
        }
        Mode::Streaming => {
            run_streaming(
                &client,
                exec,
                cli.ack_timeout,
                sender,
                shutdown,
                cli.max_inflight.max(1),
                measurement,
            )
            .await
        }
    };

    if let Ok(output) = &mut output
        && let Ok(Ok(telemetry)) =
            tokio::time::timeout(Duration::from_secs(1), read_telemetry(&client)).await
    {
        output.telemetry_end = telemetry;
    }

    output
}

#[derive(Clone, Copy)]
enum Outcome {
    Sample(SampleStatus),
    DeviceLost,
}

fn classify(res: Result<(), Failure>) -> Result<Outcome> {
    Ok(Outcome::Sample(match res {
        Ok(()) => SampleStatus::Ok,
        Err(Failure::Timeout) => SampleStatus::Timeout,
        Err(Failure::Client(ClientError::DeviceError { code, .. })) => {
            SampleStatus::DeviceError(code)
        }
        Err(Failure::Client(ClientError::Network(cause))) => {
            eprintln!("network error: {cause}");
            SampleStatus::NetworkError
        }
        Err(Failure::Client(ClientError::InvalidPayload(e))) => {
            anyhow::bail!("payload rejected by the local encoder: {e}");
        }
        Err(Failure::Client(e @ ClientError::DeviceLost { .. })) => {
            eprintln!("{e}");
            return Ok(Outcome::DeviceLost);
        }
        Err(Failure::Client(e)) => anyhow::bail!("{e}"),
    }))
}

struct Measurement {
    counters: Counters,
    traffic: Traffic,
    rtts: hdrhistogram::Histogram<u64>,
    live: Live,
    stopped_on_error: Option<(u64, SampleStatus)>,
    device_lost: bool,
    stop_on_error: bool,
    start: Instant,
    heartbeats: u64,
    ack: AckLatency,
}

impl Measurement {
    fn start(cli: &Cli, heartbeats: u64, telemetry_start: Vec<Telemetry>) -> Self {
        let traffic = Traffic::new(cli.devices.get());
        let live = Live::new(traffic, telemetry_start);
        Self {
            counters: Counters::default(),
            traffic,
            rtts: rtt_histogram(),
            live,
            stopped_on_error: None,
            device_lost: false,
            stop_on_error: cli.stop_on_error,
            start: Instant::now(),
            heartbeats,
            ack: AckLatency::default(),
        }
    }

    fn record(&mut self, index: u64, rtt: Duration, outcome: Outcome) -> ControlFlow<()> {
        let status = match outcome {
            Outcome::Sample(status) => status,
            Outcome::DeviceLost => {
                self.device_lost = true;
                SampleStatus::NetworkError
            }
        };
        self.counters.observe(status);
        if status == SampleStatus::Ok {
            record_rtt(&mut self.rtts, rtt);
            record_rtt(&mut self.live.rtts, rtt);
        }
        if self.device_lost {
            return ControlFlow::Break(());
        }
        if self.stop_on_error && status != SampleStatus::Ok {
            self.stopped_on_error = Some((index, status));
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    async fn report(&mut self, client: &Controller) {
        self.live
            .report_if_due(client, &self.counters, self.traffic, self.start)
            .await;
    }

    fn finish(self) -> RunOutput {
        let elapsed = self.start.elapsed();
        let secs = elapsed.as_secs_f64();
        let per_sec = |n: u64| if secs > 0.0 { n as f64 / secs } else { 0.0 };
        RunOutput {
            summary: Summary {
                elapsed,
                throughput_cmd_per_sec: per_sec(self.counters.ok),
                traffic: self.traffic.rate(elapsed),
                latency: latency(&self.rtts),
                frames: self.traffic.frames(),
                heartbeats: self.heartbeats,
                ack: self.ack,
                counters: self.counters,
            },
            stopped_on_error: self.stopped_on_error,
            device_lost: self.device_lost,
            telemetry_start: self.live.telemetry_start,
            telemetry_end: self.live.telemetry_last,
        }
    }
}

struct Live {
    last_report: Instant,
    block: LiveBlock,
    rtts: hdrhistogram::Histogram<u64>,
    traffic_last: Traffic,
    telemetry_start: Vec<Telemetry>,
    telemetry_last: Vec<Telemetry>,
}

impl Live {
    fn new(traffic: Traffic, telemetry_start: Vec<Telemetry>) -> Self {
        Self {
            last_report: Instant::now(),
            block: LiveBlock::new(),
            rtts: rtt_histogram(),
            traffic_last: traffic,
            telemetry_last: telemetry_start.clone(),
            telemetry_start,
        }
    }

    fn is_due(&self) -> bool {
        self.last_report.elapsed() >= Duration::from_secs(1)
    }

    async fn report_if_due(
        &mut self,
        client: &Controller,
        counters: &Counters,
        traffic: Traffic,
        start: Instant,
    ) {
        if !self.is_due() {
            return;
        }
        let interval = self.last_report.elapsed();
        self.last_report = Instant::now();
        let cmd_per_sec = self.rtts.len() as f64 / interval.as_secs_f64();
        let latency = latency(&self.rtts);
        self.rtts.reset();
        let traffic_rate = traffic.since(self.traffic_last).rate(interval);
        self.traffic_last = traffic;
        let mut lines = interval_lines(
            start.elapsed(),
            cmd_per_sec,
            counters,
            &latency,
            &traffic_rate,
        )
        .to_vec();
        match tokio::time::timeout(Duration::from_secs(1), read_telemetry(client)).await {
            Ok(Ok(now)) => {
                lines.extend(telemetry_lines(&self.telemetry_start, &now));
                self.telemetry_last = now;
            }
            Ok(Err(e)) => lines.push(format!("{:>11}telemetry  read failed: {e}", "")),
            Err(_) => lines.push(format!("{:>11}telemetry  read timed out", "")),
        }
        self.block.draw(&lines);
    }
}

async fn read_telemetry(client: &Controller) -> Result<Vec<Telemetry>, ClientError> {
    client.discard_unread_replies()?;
    let msg_id = client.send_heartbeat().await?;
    Ok(client
        .collect(msg_id)
        .await?
        .replies()
        .iter()
        .map(|r| r.telemetry)
        .collect())
}

fn spawn_signal_listener(flag: Arc<AtomicBool>) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            flag.store(true, Ordering::Relaxed);
            eprintln!("\nCtrl+C received — stopping after the current sample...");
        }
    });
}
