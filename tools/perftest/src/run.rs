use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::operation::XorHashCmd;
use autd3_rs::{
    Client, ClientConfig, CoreId, Error as ClientError, Link, LinkStats, ResponseFuture,
    StateCheck, TX_FRAME_BYTES, ThreadPriority, ThreadPriorityValue,
};
use autd3_rs_link_ethercrab::{EtherCrabLink, EtherCrabLinkOption};

use autd3_rs_link_soem::{SoemLink, SoemLinkOption};
use autd3_rs_link_twincat::{TwinCATLink, TwinCATLinkOption};

use crate::cli::{Cli, LinkKind, Mode};
use crate::mem::{self, MemProfile};
use crate::stats::{Sample, SampleStatus};

const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);
const STATE_CHECK_INTERVAL: Duration = Duration::from_millis(100);

pub struct RunOutput {
    pub samples: Vec<Sample>,
    pub warmup: u64,
    pub elapsed: Duration,
    pub frame_bytes: usize,
    pub stale_cycles: u64,
    pub lost_cycles: u64,
    pub mem: Option<MemProfile>,
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
                if Box::pin(checker.check()).await.is_err() {
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
                interface: cli.interface.clone().into(),
                sync0_period: Duration::from_micros(cli.cycle_us),
                sync0_shift: sync0_shift(cli.cycle_us, cli.shift_percent),
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
                sync0_shift: sync0_shift(cli.cycle_us, cli.shift_percent),
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
            let out = Box::pin(run_with_link(link, cli)).await;
            guard.stop().await;
            out
        }
    }
}

async fn run_with_link<L: Link>(link: L, cli: &Cli) -> Result<RunOutput> {
    let num_devices = link.num_devices();
    eprintln!("devices: {num_devices}");
    if let Some(expected) = cli.devices
        && num_devices != expected
    {
        anyhow::bail!("expected {expected} device(s) on the bus, found {num_devices}");
    }
    let link_stats = link.stats();

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
            rt_priority: cli.rt_priority.map(|p| {
                ThreadPriority::Crossplatform(
                    ThreadPriorityValue::try_from(p).expect("validated to 0..=99"),
                )
            }),
            rt_affinity: cli.rt_core.map(|id| CoreId { id }),
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

    let shutdown = Arc::new(AtomicBool::new(false));
    spawn_signal_listener(Arc::clone(&shutdown));

    let xor_cmd = XorHashCmd {
        sleep_ms: cli.sleep_ms,
        data: build_zero_xor_data(cli.data_len),
    };

    let recorder = mem::start();
    let output = match cli.mode {
        Mode::StopAndWait => run_stop_and_wait(&client, cli, &xor_cmd, shutdown, &link_stats).await,
        Mode::Streaming => {
            run_streaming(&client, cli, &xor_cmd, shutdown, max_inflight, &link_stats).await
        }
    };

    let _ = client.close().await;

    let mut output = output?;
    output.mem = mem::profile(recorder, output.samples.len() as u64);
    Ok(output)
}

async fn run_stop_and_wait(
    client: &Client,
    cli: &Cli,
    xor_cmd: &XorHashCmd,
    shutdown: Arc<AtomicBool>,
    link_stats: &LinkStats,
) -> Result<RunOutput> {
    let datagrams = client
        .datagram_builder()
        .push(xor_cmd)
        .build()
        .context("building XorHash frame")?;

    let cap = estimate_capacity(cli);
    let mut samples = Vec::with_capacity(cap);
    let mut index: u64 = 0;
    let mut progress = Progress::new(cli);

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

        let t0 = Instant::now();
        let res = client
            .send_checked(datagrams.frame(0).expect("one frame"))
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
            Err(e @ ClientError::SilencerConstraint { .. }) => {
                anyhow::bail!("rejected by the local silencer precheck: {e}");
            }
            Err(ClientError::RtClosed) => {
                eprintln!("client RT thread closed unexpectedly");
                SampleStatus::LinkError
            }
        };

        samples.push(Sample { index, rtt, status });
        index += 1;
        progress.observe(status, start.elapsed());
    }

    progress.finish();

    Ok(RunOutput {
        samples,
        warmup: cli.warmup,
        elapsed: start.elapsed(),
        frame_bytes: TX_FRAME_BYTES,
        stale_cycles: link_stats.stale_cycles(),
        lost_cycles: link_stats.lost_cycles(),
        mem: None,
    })
}

async fn run_streaming(
    client: &Client,
    cli: &Cli,
    xor_cmd: &XorHashCmd,
    shutdown: Arc<AtomicBool>,
    max_inflight: usize,
    link_stats: &LinkStats,
) -> Result<RunOutput> {
    let datagrams = client
        .datagram_builder()
        .push(xor_cmd)
        .build()
        .context("building XorHash frame")?;

    let cap = estimate_capacity(cli);
    let mut samples: Vec<Sample> = Vec::with_capacity(cap);
    let mut pending: VecDeque<PendingFuture> = VecDeque::with_capacity(max_inflight);
    let mut sends_issued: u64 = 0;
    let mut sample_index: u64 = 0;
    let mut progress = Progress::new(cli);

    let start = Instant::now();
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        let need_send = streaming_need_send(cli, sends_issued, start);

        if need_send && pending.len() < max_inflight {
            let sent_at = Instant::now();
            let fut = client.send(datagrams.frame(0).expect("one frame")).await?;
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
            Ok(resp) => match resp.data.iter().find(|&&d| d != 0) {
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
            Err(e @ ClientError::SilencerConstraint { .. }) => {
                anyhow::bail!("rejected by the local silencer precheck: {e}");
            }
            Err(ClientError::RtClosed) => {
                eprintln!("client RT thread closed unexpectedly");
                SampleStatus::LinkError
            }
        };
        samples.push(Sample {
            index: sample_index,
            rtt,
            status,
        });
        sample_index += 1;
        progress.observe(status, start.elapsed());
    }

    progress.finish();

    Ok(RunOutput {
        samples,
        warmup: cli.warmup,
        elapsed: start.elapsed(),
        frame_bytes: TX_FRAME_BYTES,
        stale_cycles: link_stats.stale_cycles(),
        lost_cycles: link_stats.lost_cycles(),
        mem: None,
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
        let progress_field = if let Some(total) = self.count_total {
            format!("{:>8}/{total}", self.completed)
        } else if let Some(total) = self.duration_total {
            format!("{:>6.1}/{:.1}s", elapsed.as_secs_f64(), total.as_secs_f64())
        } else {
            format!("{:>8} ({:.1}s)", self.completed, elapsed.as_secs_f64())
        };
        eprint!(
            "\r[{progress_field}] ok={} timeout={} dev_err={} link_err={}    ",
            self.ok, self.timeouts, self.device_errors, self.link_errors,
        );
        let _ = std::io::Write::flush(&mut std::io::stderr());
        self.rendered_once = true;
    }

    fn finish(&mut self) {
        if self.rendered_once {
            eprintln!();
        }
    }
}

fn sync0_shift(cycle_us: u64, shift_percent: u8) -> Duration {
    let nanos = u128::from(cycle_us) * 1000 * u128::from(shift_percent) / 100;
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

fn build_zero_xor_data(len: usize) -> Vec<u8> {
    if len == 0 {
        return Vec::new();
    }
    let mut data = vec![0xA5u8; len];
    let acc = data[..len - 1].iter().fold(0u8, |acc, b| acc ^ *b);
    data[len - 1] = acc;
    data
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
        return (d.as_micros() / u128::from(cli.cycle_us)) as usize;
    }
    0
}
