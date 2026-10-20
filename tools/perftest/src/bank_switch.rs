use std::net::{SocketAddr, UdpSocket};
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use autd3_cpu_wire::payload::Telemetry;
use autd3_cpu_wire::udp::{FrameReply, Header, Kind, PROTOCOL_VERSION};
use autd3_rs::commands::{
    ActivatePatternBank, Clear, ConfigPattern, GpioOut, Pattern, SetGpioOut, Synchronize,
    WritePatternBuffer,
};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::protocol::{Cmd, EXEC_AT_UNIT_NS, FrameHeader, Seq};
use autd3_rs::value::{
    Intensity, LoopBehavior, PatternBank, SamplingConfig, SysTime, TransitionMode,
};
use autd3_rs::{Datagram, DeviceState, Frames, Reply, UdpBus, UdpOption};
use zerocopy::{FromBytes, IntoBytes};

use crate::cli::{Cli, Framing};
use crate::run::{spawn_emulator, spawn_signal_listener, udp_option};
use crate::stats::{Counters, SampleStatus};

const RESET_ACK: u8 = 0xFF;
const RECV_BUFFER_BYTES: usize = 2048;
const REPORT_INTERVAL: Duration = Duration::from_secs(1);
const BIN_NS: i64 = 1_000;
const ORIGIN_NS: i64 = -10_000_000;
const HISTOGRAM_HEADROOM: Duration = Duration::from_millis(10);

pub(crate) async fn run(cli: &Cli) -> Result<()> {
    let emulator = spawn_emulator(cli)?;
    let option = udp_option(cli, emulator.as_ref());
    let shutdown = Arc::new(AtomicBool::new(false));
    spawn_signal_listener(Arc::clone(&shutdown));
    let cli = cli.clone();
    let measured = tokio::task::spawn_blocking(move || measure(&cli, &option, &shutdown))
        .await
        .context("the measurement thread panicked")?;
    drop(emulator);
    measured
}

fn exec_at(now: SysTime, lead: Duration) -> u32 {
    let us = (now + lead).sys_time().div_ceil(EXEC_AT_UNIT_NS);
    match u32::try_from(us & u64::from(u32::MAX)) {
        Ok(0) | Err(_) => 1,
        Ok(us) => us,
    }
}

fn after_boundary(sys_time: u64, exec_at: u32) -> i64 {
    let us = u32::try_from((sys_time / EXEC_AT_UNIT_NS) & u64::from(u32::MAX)).unwrap_or(0);
    let behind = i64::from(us.wrapping_sub(exec_at).cast_signed());
    behind * EXEC_AT_UNIT_NS.cast_signed() + (sys_time % EXEC_AT_UNIT_NS).cast_signed()
}

#[cfg(unix)]
fn clone_socket(bus: &UdpBus) -> std::io::Result<UdpSocket> {
    use std::os::fd::AsFd;
    Ok(UdpSocket::from(bus.as_fd().try_clone_to_owned()?))
}

#[cfg(windows)]
fn clone_socket(bus: &UdpBus) -> std::io::Result<UdpSocket> {
    use std::os::windows::io::AsSocket;
    Ok(UdpSocket::from(bus.as_socket().try_clone_to_owned()?))
}

fn stage(seq: Seq, exec_at: u32, datagram: &Datagram) -> Vec<u8> {
    let mut bytes = FrameHeader {
        seq: seq.get(),
        cmd: datagram.cmd.as_u8(),
        exec_at: exec_at.into(),
    }
    .as_bytes()
    .to_vec();
    bytes.extend_from_slice(datagram.payload());
    bytes
}

fn requests(frames: &Frames, devices: usize) -> Vec<Vec<&Datagram>> {
    frames
        .iter()
        .map(|frame| (0..devices).map(|d| frame.datagram_for(d)).collect())
        .collect()
}

struct Peeked {
    device: usize,
    msg_id: u16,
    sys_time: u64,
}

struct Answer {
    status: u8,
    ack: u8,
    sys_time: Option<u64>,
    data: Vec<u8>,
}

struct Sent {
    msg_id: u16,
    seq: Seq,
    exec_at: u32,
    answers: Vec<Option<Answer>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Status(SampleStatus),
    SeqMismatch,
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
        let answers = || answers().filter_map(|(f, a)| a.as_ref().map(|a| (f, a)));
        if answers().any(|(f, a)| a.ack != f.seq.get()) {
            return Some(Failure::SeqMismatch);
        }
        answers()
            .find(|(_, a)| a.status != 0)
            .map(|(_, a)| Failure::Status(SampleStatus::DeviceError(a.status)))
    }
}

struct Session {
    bus: UdpBus,
    peek: UdpSocket,
    buf: Box<[u8; RECV_BUFFER_BYTES]>,
    next_seq: Seq,
    ack_timeout: Duration,
    exec_lead: Option<Duration>,
    reset_rounds: NonZeroU32,
}

impl Session {
    fn open(cli: &Cli, option: &UdpOption, geometry: &Geometry) -> Result<Self> {
        let bus = UdpBus::open_with_geometry(option, geometry).context("opening the devices")?;
        let peek = clone_socket(&bus).context("duplicating the socket")?;
        Ok(Self {
            bus,
            peek,
            buf: Box::new([0; RECV_BUFFER_BYTES]),
            next_seq: Seq::ZERO,
            ack_timeout: cli.ack_timeout,
            exec_lead: cli.exec_lead,
            reset_rounds: cli.max_resync_rounds,
        })
    }

    fn device_of(&self, src: SocketAddr) -> Option<usize> {
        let SocketAddr::V6(src) = src else {
            return None;
        };
        self.bus
            .units()
            .iter()
            .position(|unit| unit.ip() == src.ip() && unit.port() == src.port())
    }

    fn peek(&mut self) -> Option<Peeked> {
        let (len, src) = self.peek.peek_from(&mut self.buf[..]).ok()?;
        let (header, body) = Header::read_from_prefix(&self.buf[..len]).ok()?;
        if header.version != PROTOCOL_VERSION || header.kind != Kind::Frame.as_u8() {
            return None;
        }
        let (reply, _) = FrameReply::read_from_prefix(body).ok()?;
        Some(Peeked {
            device: self.device_of(src)?,
            msg_id: header.msg_id.get(),
            sys_time: reply.sys_time.get(),
        })
    }

    fn receive(&mut self, deadline: Instant) -> Result<Option<(Reply, Option<u64>)>> {
        self.bus.wait_readable(deadline)?;
        let peeked = if Instant::now() < deadline {
            self.peek()
        } else {
            None
        };
        let Some(reply) = self.bus.recv(deadline)? else {
            return Ok(None);
        };
        let sys_time = peeked
            .filter(|p| p.device == reply.device && p.msg_id == reply.msg_id)
            .map(|p| p.sys_time)
            .filter(|&t| t != 0);
        Ok(Some((reply, sys_time)))
    }

    fn reset(&mut self) -> Result<()> {
        let reset = stage(Seq::ZERO, 0, &Datagram::no_payload(Cmd::Reset));
        for _ in 0..self.reset_rounds.get() {
            let sent = self.bus.broadcast(self.bus.reserve_msg_id(), &reset)?;
            let mut confirmed = vec![false; self.bus.num_devices()];
            let deadline = Instant::now() + self.ack_timeout;
            while confirmed.contains(&false) && Instant::now() < deadline {
                if let Some((reply, _)) = self.receive(deadline)?
                    && reply.msg_id == sent.msg_id
                    && reply.ack == RESET_ACK
                {
                    confirmed[reply.device] = true;
                }
            }
            if !confirmed.contains(&false) {
                self.next_seq = Seq::ZERO;
                return Ok(());
            }
        }
        bail!(
            "the devices did not acknowledge the reset in {} rounds",
            self.reset_rounds
        )
    }

    fn exchange(&mut self, requests: &[Vec<&Datagram>], scheduled: bool) -> Result<Exchange> {
        let lead = self.exec_lead.filter(|_| scheduled);
        let mut exchange = Exchange {
            started: self.bus.device_clock().now(),
            frames: Vec::with_capacity(requests.len()),
            unsent: false,
        };
        for datagrams in requests {
            let exec_at = match lead {
                None => 0,
                Some(lead) => exec_at(
                    self.bus
                        .device_clock()
                        .now()
                        .context("the device time is unknown")?,
                    lead,
                ),
            };
            let seq = self.next_seq;
            self.next_seq = seq.next();
            let bytes: Vec<Vec<u8>> = datagrams.iter().map(|d| stage(seq, exec_at, d)).collect();
            match self.bus.send(self.bus.reserve_msg_id(), &bytes) {
                Ok(sent) => {
                    if let Some(unsent) = sent.unsent {
                        eprintln!(
                            "device[{}] was not sent to: {}",
                            unsent.device, unsent.cause
                        );
                        exchange.unsent = true;
                    }
                    exchange.frames.push(Sent {
                        msg_id: sent.msg_id,
                        seq,
                        exec_at,
                        answers: datagrams.iter().map(|_| None).collect(),
                    });
                }
                Err(e) => {
                    eprintln!("sending failed: {e}");
                    exchange.unsent = true;
                    return Ok(exchange);
                }
            }
        }
        let deadline = Instant::now() + self.ack_timeout;
        while exchange
            .frames
            .iter()
            .any(|f| f.answers.iter().any(Option::is_none))
            && Instant::now() < deadline
        {
            let Some((reply, sys_time)) = self.receive(deadline)? else {
                continue;
            };
            if let Some(frame) = exchange
                .frames
                .iter_mut()
                .find(|f| f.msg_id == reply.msg_id)
            {
                frame.answers[reply.device].get_or_insert(Answer {
                    status: reply.status,
                    ack: reply.ack,
                    sys_time,
                    data: reply.data().to_vec(),
                });
            }
        }
        Ok(exchange)
    }

    fn lost_devices(&self) -> Vec<usize> {
        self.bus
            .state_checker()
            .check()
            .map(|states| {
                states
                    .devices()
                    .iter()
                    .enumerate()
                    .filter(|&(_, &state)| state == DeviceState::Lost)
                    .map(|(device, _)| device)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn run_unscheduled(&mut self, frames: &Frames, what: &str) -> Result<Exchange> {
        let devices = self.bus.num_devices();
        let exchange = self.exchange(&requests(frames, devices), false)?;
        if let Some(failure) = exchange.failure() {
            bail!("{what} failed: {failure:?}");
        }
        Ok(exchange)
    }

    fn read_telemetry(&mut self) -> Result<Vec<Telemetry>> {
        let read = Datagram::no_payload(Cmd::ReadTelemetry);
        let devices = self.bus.num_devices();
        let exchange = self.exchange(&[vec![&read; devices]], false)?;
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
    seq_mismatch: u64,
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
            seq_mismatch: 0,
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
            Failure::SeqMismatch => self.seq_mismatch += 1,
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
                    self.after_boundary[device].record(after_boundary(*time, last.exec_at));
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
        println!("  seq mismatch   : {}", self.seq_mismatch);
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

const TELEMETRY_COUNTERS: [TelemetryCounter; 8] = [
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
        name: "dedup",
        read: |t| t.dedup.get(),
        absolute: false,
    },
    TelemetryCounter {
        name: "seq_mismatch",
        read: |t| t.seq_mismatch.get(),
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

fn print_telemetry(start: &[Telemetry], end: &[Telemetry]) {
    println!();
    println!("telemetry (per device, delta over the run; sync_resync: at end):");
    for counter in &TELEMETRY_COUNTERS {
        let values: Vec<u32> = start
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

const fn other(bank: PatternBank) -> PatternBank {
    match bank {
        PatternBank::B0 => PatternBank::B1,
        PatternBank::B1 => PatternBank::B0,
    }
}

fn encode_switch(
    frames: &mut Frames,
    geometry: &Geometry,
    framing: Framing,
    bank: PatternBank,
    phases: &[Vec<autd3_rs::value::Phase>],
    intensities: &[Vec<Intensity>],
) -> Result<()> {
    match framing {
        Framing::Fused => {
            frames.encode_into(geometry, Pattern::with_bank(bank, phases, intensities))
        }
        Framing::Split => frames.encode_into(
            geometry,
            (
                WritePatternBuffer::new(bank, 0, phases, intensities),
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
    .context("encoding the switch")
}

fn prepare(
    session: &mut Session,
    geometry: &Geometry,
    phases: &[Vec<autd3_rs::value::Phase>],
    intensities: &[Vec<Intensity>],
) -> Result<()> {
    session.reset()?;
    session.run_unscheduled(&Frames::encode(geometry, Clear)?, "Clear")?;
    session.run_unscheduled(&Frames::encode(geometry, Synchronize)?, "Synchronize")?;
    session.run_unscheduled(
        &Frames::encode(
            geometry,
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
            geometry,
            Pattern::with_bank(PatternBank::B0, phases, intensities),
        )?,
        "the initial Pattern",
    )?;
    Ok(())
}

fn measure(cli: &Cli, option: &UdpOption, shutdown: &AtomicBool) -> Result<()> {
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
    let mut session = Session::open(cli, option, &geometry)?;
    let phases = geometry.phase_buffer();
    let intensities: Vec<Vec<Intensity>> = geometry
        .iter()
        .map(|d| vec![Intensity::MIN; d.num_transducers()])
        .collect();
    prepare(&mut session, &geometry, &phases, &intensities)?;
    eprintln!("GPIO[0]: PatternBank (toggles on every switch; probe it on two or more devices)");
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
        if let Err(e) = encode_switch(&mut frames, &geometry, framing, bank, &phases, &intensities)
        {
            stopped = Some(e);
            break;
        }
        let exchange = match session.exchange(&requests(&frames, devices), true) {
            Ok(exchange) => exchange,
            Err(e) => {
                stopped = Some(e);
                break;
            }
        };
        bank = other(bank);
        match exchange.failure() {
            None => stats.record(&exchange),
            Some(failure) => {
                stats.fail(failure);
                if cli.stop_on_error {
                    stopped = Some(anyhow::anyhow!(
                        "stopped at the first failure: {failure:?} (--stop-on-error)"
                    ));
                    break;
                }
                if let Err(e) = session.reset() {
                    stopped = Some(e);
                    break;
                }
            }
        }
        if last_report.elapsed() >= REPORT_INTERVAL {
            last_report = Instant::now();
            println!(
                "[{:>8.1}s] ok {}  timeout {}  seq {}  dev_err {}  net_err {}",
                start.elapsed().as_secs_f64(),
                stats.counters.ok,
                stats.counters.timeouts,
                stats.seq_mismatch,
                stats.counters.device_error_total(),
                stats.counters.network_errors,
            );
        }
    }
    let elapsed = start.elapsed();

    let telemetry_end = session.reset().and_then(|()| session.read_telemetry());
    stats.print(elapsed, cli.exec_lead.is_some());
    match &telemetry_end {
        Ok(end) => print_telemetry(&telemetry_start, end),
        Err(e) => eprintln!("reading the telemetry at the end failed: {e}"),
    }
    let lost = session.lost_devices();
    session.bus.close()?;
    match stopped {
        Some(e) if lost.is_empty() => Err(e),
        Some(e) => Err(e.context(format!("devices {lost:?} are Lost"))),
        None => Ok(()),
    }
}
