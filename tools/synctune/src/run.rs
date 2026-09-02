use std::collections::VecDeque;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs::commands::{ConfigPattern, Pattern};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Emission, Intensity, LoopBehavior, PatternBank, Phase, SamplingConfig};
use autd3_rs::{
    Client, ClientConfig, CoreId, Error as ClientError, Frames, Link, ResponseFuture, RtPriority,
    RtSchedulePolicy, StateCheck,
};
use autd3_rs_link_echocat::{EchocatLink, EchocatLinkOption, FramePhase};

use crate::cli::{Common, Mode, RtPolicy};
use crate::drift::DriftAccumulator;
use crate::grid::Candidate;
use crate::monitor::{CandidateResult, CandidateStatus, LoadStats, OpAccumulator};

pub async fn measure_candidate(
    common: &Common,
    cand: Candidate,
    shutdown: &Arc<AtomicBool>,
) -> Result<CandidateResult> {
    let period = cand.period;
    let shift = cand.shift();
    let opt = EchocatLinkOption {
        iface: common.interface.clone().into(),
        sync0_period: period,
        frame_phase: if cand.shift_percent == 0 {
            FramePhase::Auto
        } else {
            FramePhase::At(shift)
        },
        ..Default::default()
    };
    let opened = tokio::task::spawn_blocking(move || EchocatLink::open(&opt))
        .await
        .expect("open task panicked");
    match opened {
        Ok(link) => Box::pin(measure_with_link(link, common, cand, shutdown)).await,
        Err(e) => Ok(CandidateResult::failed(
            period,
            shift,
            cand.shift_percent,
            CandidateStatus::FailedOpen,
            format!("link open: {e}"),
        )),
    }
}

async fn measure_with_link<L: Link>(
    link: L,
    common: &Common,
    cand: Candidate,
    shutdown: &Arc<AtomicBool>,
) -> Result<CandidateResult> {
    let period = cand.period;
    let shift = cand.shift();
    let num_devices = link.num_devices();
    if let Some(expected) = common.devices
        && num_devices != expected
    {
        anyhow::bail!("expected {expected} device(s) on the bus, found {num_devices}");
    }

    let checker = link.state_checker();
    let dc_clock = link.dc_clock();

    let max_inflight = match common.mode {
        Mode::StopAndWait => 1,
        Mode::Streaming => common.max_inflight.max(1),
    };
    let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()).collect());
    let client = match Box::pin(Client::open(
        &geometry,
        link,
        client_config(common, max_inflight),
    ))
    .await
    {
        Ok(c) => c,
        Err(e) => {
            return Ok(CandidateResult::failed(
                period,
                shift,
                cand.shift_percent,
                CandidateStatus::Aborted,
                format!("client handshake: {e}"),
            ));
        }
    };

    let total = common.warmup + common.dwell;
    let start = Instant::now();

    let monitor = {
        let warmup = common.warmup;
        let poll = common.poll_interval;
        let shutdown = Arc::clone(shutdown);
        let mut checker = checker;
        tokio::spawn(async move {
            let mut acc = OpAccumulator::new(warmup);
            loop {
                if shutdown.load(Ordering::Relaxed) || start.elapsed() >= total {
                    break;
                }
                match checker.check() {
                    Ok(status) => acc.observe(&status, start.elapsed()),
                    Err(_) => break,
                }
                tokio::time::sleep(poll).await;
            }
            acc
        })
    };

    let sampler = spawn_drift_sampler(dc_clock, common, start, total, shutdown);

    let load = run_load(
        &client,
        &geometry,
        common,
        max_inflight,
        start,
        total,
        shutdown,
    )
    .await;

    let acc = monitor.await.expect("monitor task panicked");
    let drift = sampler.await.expect("drift sampler task panicked").finish();
    let _ = client.close().await;

    let mut result = acc.into_result(CandidateResult::new(period, shift, cand.shift_percent));
    let load = load?;
    result.send_success = load.send_success;
    result.send_errors = load.send_errors;
    result.load = load;
    result.drift = drift;
    Ok(result)
}

fn spawn_drift_sampler(
    dc_clock: Option<autd3_rs::DcClock>,
    common: &Common,
    start: Instant,
    total: Duration,
    shutdown: &Arc<AtomicBool>,
) -> tokio::task::JoinHandle<DriftAccumulator> {
    let warmup = common.warmup;
    let poll = common.poll_interval;
    let shutdown = Arc::clone(shutdown);
    tokio::spawn(async move {
        let mut acc = DriftAccumulator::new();
        let Some(dc_clock) = dc_clock else {
            return acc;
        };
        loop {
            if shutdown.load(Ordering::Relaxed) || start.elapsed() >= total {
                break;
            }
            let elapsed = start.elapsed();
            if elapsed >= warmup
                && let Some(offset_ns) = dc_clock.offset_ns()
            {
                acc.observe(elapsed.saturating_sub(warmup), offset_ns);
            }
            tokio::time::sleep(poll).await;
        }
        acc
    })
}

fn client_config(common: &Common, max_inflight: usize) -> ClientConfig {
    ClientConfig {
        timeout_cycles: common.timeout_cycles,
        max_inflight: NonZeroUsize::new(max_inflight).unwrap(),
        max_resync_rounds: common.max_resync_rounds,
        low_latency: common.low_latency,
        reset_resend_cycles: NonZeroU32::new(2).unwrap(),
        rt_priority: if common.no_rt_priority {
            None
        } else {
            match common.rt_priority {
                Some(p) => Some(RtPriority::new(p).expect("validated to 0..=99")),
                None => ClientConfig::default().rt_priority,
            }
        },
        rt_policy: match common.rt_policy {
            RtPolicy::Normal => RtSchedulePolicy::Normal,
            RtPolicy::Fifo => RtSchedulePolicy::Fifo,
            RtPolicy::RoundRobin => RtSchedulePolicy::RoundRobin,
        },
        rt_affinity: common.rt_affinity.map(|id| CoreId { id }),
        validate_state: false,
        ..Default::default()
    }
}

async fn run_load(
    client: &Client,
    geometry: &Geometry,
    common: &Common,
    max_inflight: usize,
    start: Instant,
    total: Duration,
    shutdown: &Arc<AtomicBool>,
) -> Result<LoadStats> {
    send_config_pattern_once(client)
        .await
        .context("initial ConfigPattern")?;
    let mut emissions = geometry.pattern_buffer();
    fill_emissions(&mut emissions);
    let frames = client
        .datagram_builder()
        .push(Pattern::with_bank(PatternBank::B0, &emissions))
        .build()
        .context("building Pattern frame")?;

    let warmup = common.warmup;
    match common.mode {
        Mode::StopAndWait => {
            load_stop_and_wait(client, &frames, start, total, warmup, shutdown).await
        }
        Mode::Streaming => {
            load_streaming(
                client,
                &frames,
                start,
                total,
                warmup,
                shutdown,
                max_inflight,
            )
            .await
        }
    }
}

struct LoadAcc {
    warmup: Duration,
    stats: LoadStats,
}

impl LoadAcc {
    fn new(warmup: Duration) -> Self {
        Self {
            warmup,
            stats: LoadStats::default(),
        }
    }

    fn record(&mut self, ok: bool, completed_at: Duration) {
        if ok {
            self.stats.send_success += 1;
            if completed_at >= self.warmup {
                self.stats.success_in_window += 1;
            }
        } else {
            self.stats.send_errors += 1;
        }
    }

    fn finish(mut self, total_elapsed: Duration) -> LoadStats {
        self.stats.window = total_elapsed.saturating_sub(self.warmup);
        self.stats
    }
}

async fn load_stop_and_wait(
    client: &Client,
    frames: &Frames,
    start: Instant,
    total: Duration,
    warmup: Duration,
    shutdown: &Arc<AtomicBool>,
) -> Result<LoadStats> {
    let mut acc = LoadAcc::new(warmup);
    loop {
        if shutdown.load(Ordering::Relaxed) || start.elapsed() >= total {
            break;
        }
        match client
            .send_checked(frames.frame(0).expect("one frame"))
            .await
        {
            Ok(()) => acc.record(true, start.elapsed()),
            Err(ClientError::InvalidPayload(msg)) => {
                anyhow::bail!("payload rejected by the local encoder: {msg}")
            }
            Err(_) => acc.record(false, start.elapsed()),
        }
    }
    Ok(acc.finish(start.elapsed()))
}

async fn load_streaming(
    client: &Client,
    frames: &Frames,
    start: Instant,
    total: Duration,
    warmup: Duration,
    shutdown: &Arc<AtomicBool>,
    max_inflight: usize,
) -> Result<LoadStats> {
    let mut acc = LoadAcc::new(warmup);
    let mut pending: VecDeque<ResponseFuture> = VecDeque::with_capacity(max_inflight);
    loop {
        let stop = shutdown.load(Ordering::Relaxed) || start.elapsed() >= total;
        if !stop && pending.len() < max_inflight {
            match client.send(frames.frame(0).expect("one frame")).await {
                Ok(fut) => pending.push_back(fut),
                Err(ClientError::InvalidPayload(msg)) => {
                    anyhow::bail!("payload rejected by the local encoder: {msg}")
                }
                Err(_) => acc.record(false, start.elapsed()),
            }
            continue;
        }
        let Some(fut) = pending.pop_front() else {
            break;
        };
        match fut.await {
            Ok(_) => acc.record(true, start.elapsed()),
            Err(ClientError::InvalidPayload(msg)) => {
                anyhow::bail!("payload rejected by the local encoder: {msg}")
            }
            Err(_) => acc.record(false, start.elapsed()),
        }
    }
    Ok(acc.finish(start.elapsed()))
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

fn fill_emissions(emissions: &mut [Vec<Emission>]) {
    for device in emissions {
        let mut phase = 0u8;
        for e in device.iter_mut() {
            e.phase = Phase(phase);
            e.intensity = Intensity::MIN;
            phase = phase.wrapping_add(1);
        }
    }
}
