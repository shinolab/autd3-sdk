use std::collections::VecDeque;
use std::net::{Ipv6Addr, SocketAddrV6};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs::commands::{
    ActivatePatternBank, Clear, Command as AutdCommand, ConfigPattern, Expansion, GpioOut, Nop,
    Pattern, SetGpioOut, Synchronize, WriteModulationBuffer, WritePatternBuffer,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::params::MOD_BUFFER_SAMPLES;
use autd3_rs::value::{
    Intensity, LoopBehavior, ModulationBank, PatternBank, Phase, SamplingConfig, TransitionMode,
};
use autd3_rs::{
    Client, Error as ClientError, ExecAt, Frame, Frames, Interface, ResponseFuture, Telemetry,
    TransportOption,
};
use autd3_rs_firmware_emulator::udp::UdpEmulator;

use crate::cli::{Cli, Command, Mode};
use crate::report::{LiveBlock, interval_lines, telemetry_lines};
use crate::stats::{AckLatency, Counters, Histogram, SampleStatus, Summary, Traffic};

const REPORT_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) struct RunOutput {
    pub summary: Summary,
    pub stopped_on_error: Option<(u64, SampleStatus)>,
    pub driver_closed: bool,
    pub telemetry_start: Vec<Telemetry>,
    pub telemetry_end: Vec<Telemetry>,
}

struct WholeModulationWrite<'a>(&'a [u8]);

impl AutdCommand for WholeModulationWrite<'_> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), ClientError> {
        for (i, data) in self.0.chunks(WriteModulationBuffer::MAX_LEN).enumerate() {
            expansion.push(WriteModulationBuffer {
                bank: ModulationBank::B1,
                offset: i * WriteModulationBuffer::MAX_LEN,
                data,
            })?;
        }
        Ok(())
    }
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
                .encode_into(client.geometry(), WholeModulationWrite(&self.modulation))
                .context("encoding modulation write")?;
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
}

pub(crate) fn spawn_emulator(cli: &Cli) -> Result<Option<UdpEmulator>> {
    cli.emulator
        .then(|| UdpEmulator::spawn(cli.devices.get()))
        .transpose()
        .context("starting the UDP device emulator")
}

pub(crate) fn transport_option(cli: &Cli, emulator: Option<&UdpEmulator>) -> TransportOption {
    let mut option = match emulator {
        Some(emulator) => TransportOption {
            iface: emulator.interface(),
            open_timeout: Duration::from_secs(6),
            ..Default::default()
        },
        None => TransportOption {
            iface: match cli.simulator {
                Some(port) => Interface::Addr(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0)),
                None => cli
                    .interface
                    .clone()
                    .map_or(Interface::Auto, Interface::Name),
            },
            ..Default::default()
        },
    };
    option.ack_timeout = cli.ack_timeout;
    if let Some(bytes) = cli.send_buffer {
        option.send_buffer_bytes = std::num::NonZeroUsize::new(bytes);
    }
    option
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn run(cli: &Cli) -> Result<RunOutput> {
    #[allow(clippy::too_many_lines)]
    async fn run_with_option(option: &TransportOption, cli: &Cli) -> Result<RunOutput> {
        async fn send_config_pattern_once(client: &Client) -> Result<()> {
            client
                .send(ConfigPattern {
                    bank: PatternBank::B0,
                    config: SamplingConfig::FREQ_4K,
                    size: 1,
                    loop_behavior: LoopBehavior::Infinite,
                })
                .await?;
            client
                .send(ActivatePatternBank {
                    bank: PatternBank::B0,
                    transition_mode: TransitionMode::Immediate,
                })
                .await?;
            Ok(())
        }

        async fn send_base_signal_gpio_out_once(client: &Client) -> Result<()> {
            client
                .send(SetGpioOut {
                    outputs: [
                        GpioOut::BaseSignal,
                        GpioOut::Off,
                        GpioOut::Off,
                        GpioOut::Off,
                    ],
                })
                .await?;
            Ok(())
        }

        async fn run_stop_and_wait(
            client: &Client,
            cli: &Cli,
            exec: ExecAt,
            mut sender: Sender,
            shutdown: Arc<AtomicBool>,
            mut measurement: Measurement,
        ) -> Result<RunOutput> {
            async fn send_all(
                client: &Client,
                exec: ExecAt,
                frames: &Frames,
                window: usize,
                traffic: &mut Traffic,
                ack: &mut AckLatency,
            ) -> Result<(), ClientError> {
                let mut pending: VecDeque<PendingFuture> = VecDeque::with_capacity(window);
                for frame in frames {
                    if pending.len() >= window {
                        let entry = pending.pop_front().expect("non-empty");
                        entry.acked(ack).await?.check()?;
                        traffic.replied();
                    }
                    let sent_at = Instant::now();
                    let fut = client.send_frame_at(frame, exec).await?;
                    pending.push_back(PendingFuture { sent_at, fut });
                    traffic.sent(&frame);
                }
                for entry in pending {
                    entry.acked(ack).await?.check()?;
                    traffic.replied();
                }
                Ok(())
            }

            let window = if cli.command.is_bulk() {
                cli.max_inflight.max(1)
            } else {
                1
            };
            let mut index: u64 = 0;
            while !shutdown.load(Ordering::Relaxed) {
                sender.prepare(client)?;
                let t0 = Instant::now();
                let res = send_all(
                    client,
                    exec,
                    &sender.frames,
                    window,
                    &mut measurement.traffic,
                    &mut measurement.ack,
                )
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
            client: &Client,
            exec: ExecAt,
            mut sender: Sender,
            shutdown: Arc<AtomicBool>,
            max_inflight: usize,
            mut measurement: Measurement,
        ) -> Result<RunOutput> {
            let mut pending: VecDeque<PendingFuture> = VecDeque::with_capacity(max_inflight);
            let mut sample_index: u64 = 0;
            while !shutdown.load(Ordering::Relaxed) {
                if pending.len() < max_inflight {
                    let frame = sender.next_frame(client)?;
                    let sent_at = Instant::now();
                    let fut = match client.send_frame_at(frame, exec).await {
                        Ok(fut) => fut,
                        Err(ClientError::Closed) => {
                            eprintln!("the client closed unexpectedly");
                            measurement.driver_closed = true;
                            break;
                        }
                        Err(e) => return Err(e.into()),
                    };
                    measurement.traffic.sent(&frame);
                    pending.push_back(PendingFuture { sent_at, fut });
                    continue;
                }

                let Some(entry) = pending.pop_front() else {
                    break;
                };
                let sent_at = entry.sent_at;
                let res = entry.acked(&mut measurement.ack).await;
                let rtt = sent_at.elapsed();
                let outcome = classify(res.and_then(|resp| resp.check()))?;
                if matches!(outcome, Outcome::Sample(SampleStatus::Ok)) {
                    measurement.traffic.replied();
                }
                if measurement.record(sample_index, rtt, outcome).is_break() {
                    break;
                }
                measurement.report(client).await;
                sample_index += 1;
            }
            Ok(measurement.finish())
        }

        let num_devices = cli.devices.get();
        eprintln!("devices: {num_devices}");

        let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()));
        let client = Client::open(&geometry, option).context("opening the devices")?;
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
            send_config_pattern_once(&client)
                .await
                .context("initial ConfigPattern + ActivatePatternBank")?;
        }
        send_base_signal_gpio_out_once(&client)
            .await
            .context("initial SetGpioOut")?;
        eprintln!("GPIO[0]: BaseSignal (probe it to check inter-device sync)");

        let shutdown = Arc::new(AtomicBool::new(false));
        spawn_signal_listener(Arc::clone(&shutdown));

        let telemetry_start = client.read_telemetry().await.context("reading telemetry")?;
        let mut heartbeats = 0;
        if let Some(hold) = cli.hold {
            eprintln!("holding the connection idle for {hold:?}");
            let until = tokio::time::Instant::now() + hold;
            let mut heartbeat = tokio::time::interval(cli.heartbeat);
            while tokio::time::Instant::now() < until {
                tokio::select! {
                    _ = heartbeat.tick() => {
                        client.send_heartbeat().context("sending a heartbeat")?;
                        heartbeats += 1;
                    }
                    () = tokio::time::sleep_until(until) => {}
                }
            }
        }

        let sender = Sender::new(&geometry, cli)?;

        let measurement = Measurement::start(cli, heartbeats, telemetry_start);
        let exec = cli.exec_lead.map_or(ExecAt::Immediate, ExecAt::Lead);
        let mut output = match cli.mode {
            Mode::StopAndWait => {
                run_stop_and_wait(&client, cli, exec, sender, shutdown, measurement).await
            }
            Mode::Streaming => {
                run_streaming(
                    &client,
                    exec,
                    sender,
                    shutdown,
                    cli.max_inflight.max(1),
                    measurement,
                )
                .await
            }
            Mode::BankSwitch => unreachable!("bank-switch runs without the Client"),
        };

        if let Ok(output) = &mut output
            && let Ok(telemetry) = client.read_telemetry().await
        {
            output.telemetry_end = telemetry;
        }

        if let Err(e) = client.close() {
            eprintln!("the client closed with an error: {e}");
        }

        output
    }

    let emulator = spawn_emulator(cli)?;
    let option = transport_option(cli, emulator.as_ref());
    let out = Box::pin(run_with_option(&option, cli)).await;
    drop(emulator);
    out
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
        Err(ClientError::Closed) => {
            eprintln!("the client closed unexpectedly");
            return Ok(Outcome::DriverClosed);
        }
        Err(e) => anyhow::bail!("{e}"),
    }))
}

struct Measurement {
    counters: Counters,
    traffic: Traffic,
    rtts: Histogram,
    live: Live,
    stopped_on_error: Option<(u64, SampleStatus)>,
    driver_closed: bool,
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
            rtts: Histogram::new(),
            live,
            stopped_on_error: None,
            driver_closed: false,
            stop_on_error: cli.stop_on_error,
            start: Instant::now(),
            heartbeats,
            ack: AckLatency::default(),
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
        self.counters.observe(status);
        if status == SampleStatus::Ok {
            self.rtts.record(rtt);
            self.live.rtts.record(rtt);
        }
        if self.driver_closed {
            return ControlFlow::Break(());
        }
        if self.stop_on_error && status != SampleStatus::Ok {
            self.stopped_on_error = Some((index, status));
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    async fn report(&mut self, client: &Client) {
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
                latency: self.rtts.latency(),
                frames: self.traffic.frames(),
                heartbeats: self.heartbeats,
                ack: self.ack,
                counters: self.counters,
            },
            stopped_on_error: self.stopped_on_error,
            driver_closed: self.driver_closed,
            telemetry_start: self.live.telemetry_start,
            telemetry_end: self.live.telemetry_last,
        }
    }
}

struct PendingFuture {
    sent_at: Instant,
    fut: ResponseFuture,
}

impl PendingFuture {
    async fn acked(self, ack: &mut AckLatency) -> Result<autd3_rs::Response, ClientError> {
        let response = self.fut.await?;
        ack.record(self.sent_at.elapsed());
        Ok(response)
    }
}

struct Live {
    last_report: Instant,
    block: LiveBlock,
    rtts: Histogram,
    traffic_last: Traffic,
    telemetry_start: Vec<Telemetry>,
    telemetry_last: Vec<Telemetry>,
}

impl Live {
    fn new(traffic: Traffic, telemetry_start: Vec<Telemetry>) -> Self {
        Self {
            last_report: Instant::now(),
            block: LiveBlock::new(),
            rtts: Histogram::new(),
            traffic_last: traffic,
            telemetry_last: telemetry_start.clone(),
            telemetry_start,
        }
    }

    async fn report_if_due(
        &mut self,
        client: &Client,
        counters: &Counters,
        traffic: Traffic,
        start: Instant,
    ) {
        let interval = self.last_report.elapsed();
        if interval < REPORT_INTERVAL {
            return;
        }
        self.last_report = Instant::now();
        let cmd_per_sec = self.rtts.count() as f64 / interval.as_secs_f64();
        let latency = self.rtts.latency();
        self.rtts.clear();
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
        match client.read_telemetry().await {
            Ok(now) => {
                lines.extend(telemetry_lines(&self.telemetry_start, &now));
                self.telemetry_last = now;
            }
            Err(e) => lines.push(format!("{:>11}telemetry  read failed: {e}", "")),
        }
        self.block.draw(&lines);
    }
}

pub(crate) fn spawn_signal_listener(flag: Arc<AtomicBool>) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            flag.store(true, Ordering::Relaxed);
            eprintln!("\nCtrl+C received — stopping after the current sample...");
        }
    });
}
