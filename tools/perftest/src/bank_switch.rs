use std::num::NonZeroU16;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use autd3_cpu_wire::payload::Telemetry;
use autd3_rs::commands::{
    ActivatePatternBank, Clear, ConfigPattern, Distribution, GpioOut, Pattern, SetGpioOut,
    Synchronize, WritePatternBuffer,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::protocol::{Cmd, EXEC_AT_UNIT_NS};
use autd3_rs::value::{
    Intensity, LoopBehavior, PatternBank, SamplingConfig, SysTime, TransitionMode,
};
use autd3_rs::{
    Client, Datagram, DeviceState, Error, ExecAt, Frame, Frames, Response, TransportOption,
};
use pollster::block_on;
use zerocopy::FromBytes;

use crate::cli::{Cli, Framing};
use crate::run::{spawn_emulator, spawn_signal_listener, transport_option};
use crate::stats::{Counters, SampleStatus};

const REPORT_INTERVAL: Duration = Duration::from_secs(1);
const BIN_NS: i64 = 1_000;
const ORIGIN_NS: i64 = -10_000_000;
const HISTOGRAM_HEADROOM: Duration = Duration::from_millis(10);

#[allow(clippy::too_many_lines)]
pub(crate) async fn run(cli: &Cli) -> Result<()> {
    #[allow(clippy::too_many_lines)]
    fn measure(cli: &Cli, option: &TransportOption, shutdown: &AtomicBool) -> Result<()> {
        let devices = cli.devices.get();
        let framing = cli.framing.unwrap_or_default();
        eprintln!(
            "devices: {devices}  framing: {framing:?}  exec_lead: {:?}",
            cli.exec_lead
        );
        if cli.ack_timeout >= option.lost_timeout {
            bail!(
                "--ack-timeout {:?} must be shorter than the lost timeout {:?}",
                cli.ack_timeout,
                option.lost_timeout
            );
        }
        let geometry = Geometry::new((0..devices).map(|_| Autd3::default()));
        let session = Session::open(cli, option, &geometry)?;
        let patterns = [geometry.phase_buffer()];
        let intensities: Vec<Vec<Intensity>> = geometry
            .iter()
            .map(|d| vec![Intensity::MIN; d.num_transducers()])
            .collect();
        session.run_unscheduled(&Frames::encode(&geometry, Clear)?, "Clear")?;
        session.run_unscheduled(&Frames::encode(&geometry, Synchronize)?, "Synchronize")?;
        session.run_unscheduled(
            &Frames::encode(
                &geometry,
                SetGpioOut {
                    outputs: [
                        GpioOut::PatternBank,
                        GpioOut::Off,
                        GpioOut::Off,
                        GpioOut::Off,
                    ],
                },
            )?,
            "SetGpioOut",
        )?;
        session.run_unscheduled(
            &Frames::encode(
                &geometry,
                Pattern::with_bank(PatternBank::B0, &patterns[0], &intensities),
            )?,
            "the initial Pattern",
        )?;
        eprintln!(
            "GPIO[0]: PatternBank (toggles on every switch; probe it on two or more devices)"
        );
        let telemetry_start = session.read_telemetry()?;

        let upper_ns =
            i64::try_from((cli.ack_timeout + HISTOGRAM_HEADROOM).as_nanos()).unwrap_or(i64::MAX);
        let bins = usize::try_from((upper_ns - ORIGIN_NS) / BIN_NS).unwrap_or(0);
        let mut stats = Stats::new(devices, bins);
        let mut frames = Frames::default();
        let mut bank = PatternBank::B1;
        let mut stopped: Option<anyhow::Error> = None;
        let start = Instant::now();
        let mut last_report = start;
        while !shutdown.load(Ordering::Relaxed) {
            let encoded = match framing {
                Framing::Fused => frames.encode_into(
                    &geometry,
                    Pattern::with_bank(bank, &patterns[0], &intensities),
                ),
                Framing::Split => frames.encode_into(
                    &geometry,
                    (
                        WritePatternBuffer::new(bank, 0, &patterns, &intensities),
                        ConfigPattern {
                            bank,
                            config: SamplingConfig::new(NonZeroU16::MAX),
                            size: 1,
                            loop_behavior: LoopBehavior::Infinite,
                        },
                        ActivatePatternBank {
                            bank,
                            transition_mode: TransitionMode::Immediate,
                        },
                    ),
                ),
            }
            .context("encoding the switch");
            if let Err(e) = encoded {
                stopped = Some(e);
                break;
            }
            let exchange = match session.exchange(&frames, true) {
                Ok(exchange) => exchange,
                Err(e) => {
                    stopped = Some(e);
                    break;
                }
            };
            bank = match bank {
                PatternBank::B0 => PatternBank::B1,
                PatternBank::B1 => PatternBank::B0,
            };
            match exchange.failure() {
                None => stats.record(&exchange),
                Some(failure) => {
                    stats.fail(failure);
                    let lost = session.lost_devices();
                    if !lost.is_empty() {
                        stopped = Some(anyhow::anyhow!(
                            "stopped because devices {lost:?} are Lost: the client no longer sends to them"
                        ));
                        break;
                    }
                    if cli.stop_on_error {
                        stopped = Some(anyhow::anyhow!(
                            "stopped at the first failure: {failure:?} (--stop-on-error)"
                        ));
                        break;
                    }
                }
            }
            if last_report.elapsed() >= REPORT_INTERVAL {
                last_report = Instant::now();
                println!(
                    "[{:>8.1}s] ok {}  timeout {}  dev_err {}  net_err {}",
                    start.elapsed().as_secs_f64(),
                    stats.counters.ok,
                    stats.counters.timeouts,
                    stats.counters.device_error_total(),
                    stats.counters.network_errors,
                );
            }
        }
        let elapsed = start.elapsed();

        let telemetry_end = session.read_telemetry();
        stats.print(elapsed, cli.exec_lead.is_some());
        match &telemetry_end {
            Ok(end) => {
                println!();
                println!("telemetry (per device, delta over the run; sync_resync: at end):");
                for counter in &TELEMETRY_COUNTERS {
                    let values: Vec<u32> = telemetry_start
                        .iter()
                        .zip(end)
                        .map(|(s, e)| {
                            if counter.absolute {
                                (counter.read)(e)
                            } else {
                                (counter.read)(e).wrapping_sub((counter.read)(s))
                            }
                        })
                        .collect();
                    println!("  {:<15}: {values:?}", counter.name);
                }
            }
            Err(e) => eprintln!("reading the telemetry at the end failed: {e}"),
        }
        let lost = session.lost_devices();
        drop(session);
        match stopped {
            Some(e) if lost.is_empty() => Err(e),
            Some(e) => Err(e.context(format!("devices {lost:?} are Lost"))),
            None => Ok(()),
        }
    }

    let emulator = spawn_emulator(cli)?;
    let option = transport_option(cli, emulator.as_ref());
    let shutdown = Arc::new(AtomicBool::new(false));
    spawn_signal_listener(Arc::clone(&shutdown));
    let cli = cli.clone();
    let measured = tokio::task::spawn_blocking(move || measure(&cli, &option, &shutdown))
        .await
        .context("the measurement thread panicked")?;
    drop(emulator);
    measured
}

struct Answer {
    status: u8,
    sys_time: Option<u64>,
    data: Vec<u8>,
}

struct Sent {
    exec_at: u32,
    answers: Vec<Option<Answer>>,
}

impl Sent {
    fn answered(exec_at: u32, response: &Response, lost: &[usize]) -> Self {
        Self {
            exec_at,
            answers: (0..response.error_codes().len())
                .map(|device| {
                    (!lost.contains(&device)).then(|| Answer {
                        status: response.error_codes()[device],
                        sys_time: response.sys_time_of(device).map(SysTime::sys_time),
                        data: response.reply_data_of(device).to_vec(),
                    })
                })
                .collect(),
        }
    }

    fn unanswered(exec_at: u32, devices: usize) -> Self {
        Self {
            exec_at,
            answers: (0..devices).map(|_| None).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Failure {
    Status(SampleStatus),
}

struct Exchange {
    started: Option<SysTime>,
    frames: Vec<Sent>,
    unsent: bool,
}

impl Exchange {
    fn failure(&self) -> Option<Failure> {
        if self.unsent {
            return Some(Failure::Status(SampleStatus::NetworkError));
        }
        let answers = || {
            self.frames
                .iter()
                .flat_map(|f| f.answers.iter().map(move |a| (f, a)))
        };
        if answers().any(|(_, a)| a.is_none()) {
            return Some(Failure::Status(SampleStatus::Timeout));
        }
        answers()
            .filter_map(|(_, a)| a.as_ref())
            .find(|a| a.status != 0)
            .map(|a| Failure::Status(SampleStatus::DeviceError(a.status)))
    }
}

struct Session {
    client: Client,
    exec_lead: Option<Duration>,
}

impl Session {
    fn open(cli: &Cli, option: &TransportOption, geometry: &Geometry) -> Result<Self> {
        let client = Client::open(geometry, option).context("opening the devices")?;
        Ok(Self {
            client,
            exec_lead: cli.exec_lead,
        })
    }

    fn exchange<'a>(
        &self,
        frames: impl IntoIterator<Item = Frame<'a>>,
        scheduled: bool,
    ) -> Result<Exchange> {
        let lead = self.exec_lead.filter(|_| scheduled);
        let devices = self.client.num_devices();
        let mut exchange = Exchange {
            started: self.client.device_time_now().ok(),
            frames: Vec::new(),
            unsent: false,
        };
        let mut pending = Vec::new();
        for frame in frames {
            let (exec, exec_at) = match lead {
                None => (ExecAt::Immediate, 0),
                Some(lead) => {
                    let at = self
                        .client
                        .device_time_now()
                        .context("the device time is unknown")?
                        + lead;
                    let us = at.sys_time().div_ceil(EXEC_AT_UNIT_NS);
                    let stamp = match u32::try_from(us & u64::from(u32::MAX)) {
                        Ok(0) | Err(_) => 1,
                        Ok(us) => us,
                    };
                    (ExecAt::At(at), stamp)
                }
            };
            match block_on(self.client.send_frame_at(frame, exec)) {
                Ok(response) => pending.push((exec_at, response)),
                Err(e) => {
                    eprintln!("sending failed: {e}");
                    exchange.unsent = true;
                    return Ok(exchange);
                }
            }
        }
        for (exec_at, response) in pending {
            let sent = match block_on(response) {
                Ok(response) => Sent::answered(exec_at, &response, &self.lost_devices()),
                Err(Error::Timeout { .. }) => Sent::unanswered(exec_at, devices),
                Err(e) => {
                    eprintln!("waiting for the replies failed: {e}");
                    exchange.unsent = true;
                    Sent::unanswered(exec_at, devices)
                }
            };
            exchange.frames.push(sent);
        }
        Ok(exchange)
    }

    fn lost_devices(&self) -> Vec<usize> {
        self.client
            .device_status()
            .map(|status| {
                status
                    .iter()
                    .enumerate()
                    .filter(|(_, status)| status.state == DeviceState::Lost)
                    .map(|(device, _)| device)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn run_unscheduled(&self, frames: &Frames, what: &str) -> Result<Exchange> {
        let exchange = self.exchange(frames, false)?;
        if let Some(failure) = exchange.failure() {
            bail!("{what} failed: {failure:?}");
        }
        Ok(exchange)
    }

    fn read_telemetry(&self) -> Result<Vec<Telemetry>> {
        let read = [Datagram::no_payload(Cmd::ReadTelemetry)];
        let exchange = self.exchange([Frame::new(Distribution::Broadcast, &read)], false)?;
        if let Some(failure) = exchange.failure() {
            bail!("reading the telemetry failed: {failure:?}");
        }
        exchange.frames[0]
            .answers
            .iter()
            .flatten()
            .enumerate()
            .map(|(device, answer)| {
                Telemetry::read_from_prefix(&answer.data)
                    .map(|(t, _)| t)
                    .map_err(|_| anyhow::anyhow!("device[{device}] sent a short telemetry"))
            })
            .collect()
    }
}

struct LinearHistogram {
    bins: Vec<u32>,
    below: u64,
    count: u64,
    min: i64,
    max: i64,
}

impl LinearHistogram {
    fn new(bins: usize) -> Self {
        Self {
            bins: vec![0; bins],
            below: 0,
            count: 0,
            min: i64::MAX,
            max: i64::MIN,
        }
    }

    fn record(&mut self, ns: i64) {
        match usize::try_from((ns - ORIGIN_NS).div_euclid(BIN_NS)) {
            Ok(index) if index < self.bins.len() => self.bins[index] += 1,
            Ok(_) => {}
            Err(_) => self.below += 1,
        }
        self.count += 1;
        self.min = self.min.min(ns);
        self.max = self.max.max(ns);
    }

    fn percentile(&self, p: u64) -> i64 {
        let rank = (p * self.count).div_ceil(100).max(1);
        let mut seen = self.below;
        if seen >= rank {
            return self.min;
        }
        for (index, &n) in self.bins.iter().enumerate() {
            seen += u64::from(n);
            if seen >= rank {
                let upper = ORIGIN_NS + (i64::try_from(index).unwrap_or(i64::MAX) + 1) * BIN_NS;
                return upper.clamp(self.min, self.max);
            }
        }
        self.max
    }

    fn line(&self) -> String {
        if self.count == 0 {
            return "no sample".to_string();
        }
        format!(
            "min {:>9}  p50 {:>9}  p99 {:>9}  max {:>9}  (n {})",
            us(self.min),
            us(self.percentile(50)),
            us(self.percentile(99)),
            us(self.max),
            self.count
        )
    }
}

fn us(ns: i64) -> String {
    format!("{:.1} us", ns as f64 / 1e3)
}

struct Stats {
    counters: Counters,
    unknown_start: u64,
    missing_sys_time: u64,
    delay: Vec<LinearHistogram>,
    after_boundary: Vec<LinearHistogram>,
    spread: LinearHistogram,
}

impl Stats {
    fn new(devices: usize, bins: usize) -> Self {
        Self {
            counters: Counters::default(),
            unknown_start: 0,
            missing_sys_time: 0,
            delay: (0..devices).map(|_| LinearHistogram::new(bins)).collect(),
            after_boundary: (0..devices).map(|_| LinearHistogram::new(bins)).collect(),
            spread: LinearHistogram::new(bins),
        }
    }

    fn fail(&mut self, failure: Failure) {
        match failure {
            Failure::Status(status) => self.counters.observe(status),
        }
    }

    fn record(&mut self, exchange: &Exchange) {
        self.counters.observe(SampleStatus::Ok);
        let Some(last) = exchange.frames.last() else {
            return;
        };
        let times: Vec<Option<u64>> = last
            .answers
            .iter()
            .map(|a| a.as_ref().and_then(|a| a.sys_time))
            .collect();
        if let Some(started) = exchange.started {
            for (device, time) in times.iter().enumerate() {
                if let Some(time) = time {
                    self.delay[device]
                        .record(time.cast_signed() - started.sys_time().cast_signed());
                }
            }
        } else {
            self.unknown_start += 1;
        }
        for (device, time) in times.iter().enumerate() {
            match time {
                Some(time) if last.exec_at != 0 => {
                    let us =
                        u32::try_from((time / EXEC_AT_UNIT_NS) & u64::from(u32::MAX)).unwrap_or(0);
                    let behind = i64::from(us.wrapping_sub(last.exec_at).cast_signed());
                    self.after_boundary[device].record(
                        behind * EXEC_AT_UNIT_NS.cast_signed()
                            + (time % EXEC_AT_UNIT_NS).cast_signed(),
                    );
                }
                Some(_) => {}
                None => self.missing_sys_time += 1,
            }
        }
        if let Some(times) = times.iter().copied().collect::<Option<Vec<u64>>>()
            && let (Some(min), Some(max)) = (times.iter().min(), times.iter().max())
        {
            self.spread.record((max - min).cast_signed());
        }
    }

    fn print(&self, elapsed: Duration, scheduled: bool) {
        let c = &self.counters;
        println!();
        println!("=== bank-switch summary ===");
        println!("elapsed          : {:.1} s", elapsed.as_secs_f64());
        println!(
            "switches         : {} ok ({:.1} /s)",
            c.ok,
            c.ok as f64 / elapsed.as_secs_f64().max(f64::MIN_POSITIVE)
        );
        println!("  timeouts       : {}", c.timeouts);
        println!("  network errors : {}", c.network_errors);
        for (code, count) in &c.device_errors {
            println!("  device error {code:#04x}: {count}");
        }
        println!("  device time unknown at the send : {}", self.unknown_start);
        println!(
            "  replies without a device time   : {}",
            self.missing_sys_time
        );
        println!();
        println!(
            "delay: reply device time of the switching frame - device time right before the send"
        );
        for (device, h) in self.delay.iter().enumerate() {
            println!("  device[{device:>2}]  {}", h.line());
        }
        if scheduled {
            println!();
            println!("after the boundary: reply device time - EXEC_AT of the switching frame");
            for (device, h) in self.after_boundary.iter().enumerate() {
                println!("  device[{device:>2}]  {}", h.line());
            }
        }
        println!();
        println!("spread: latest - earliest reply device time of one switch across the devices");
        println!("              {}", self.spread.line());
    }
}

struct TelemetryCounter {
    name: &'static str,
    read: fn(&Telemetry) -> u32,
    absolute: bool,
}

const TELEMETRY_COUNTERS: [TelemetryCounter; 6] = [
    TelemetryCounter {
        name: "exec_late",
        read: |t| t.exec_late.get(),
        absolute: false,
    },
    TelemetryCounter {
        name: "fifo_drop",
        read: |t| t.fifo_drop.get(),
        absolute: false,
    },
    TelemetryCounter {
        name: "dispatch_error",
        read: |t| t.dispatch_error.get(),
        absolute: false,
    },
    TelemetryCounter {
        name: "send_failure",
        read: |t| t.send_failure.get(),
        absolute: false,
    },
    TelemetryCounter {
        name: "failsafe",
        read: |t| u32::from(t.failsafe.get()),
        absolute: false,
    },
    TelemetryCounter {
        name: "sync_resync",
        read: |t| u32::from(t.sync_resync),
        absolute: true,
    },
];
