use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::operation::XorHashCmd;
use autd3_rs::{
    Client, ClientConfig, CoreId, Datagrams, Error as ClientError, Link, ResponseFuture,
    StateCheck, ThreadPriority, ThreadPriorityValue,
};
use autd3_rs_link_ethercrab::{EtherCrabLink, EtherCrabLinkOption};
use autd3_rs_link_soem::{SoemLink, SoemLinkOption};

use crate::cli::{Common, LinkKind, Mode};
use crate::grid::Candidate;
use crate::monitor::{CandidateResult, CandidateStatus, OpAccumulator};

pub async fn measure_candidate(
    common: &Common,
    cand: Candidate,
    shutdown: &Arc<AtomicBool>,
) -> Result<CandidateResult> {
    let period = cand.period;
    let shift = cand.shift();
    match common.link {
        LinkKind::Ethercrab => {
            let opt = EtherCrabLinkOption {
                interface: common.interface.clone().into(),
                sync0_period: period,
                sync0_shift: shift,
                ..Default::default()
            };
            match Box::pin(EtherCrabLink::open(opt)).await {
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
        LinkKind::Soem => {
            let opt = SoemLinkOption {
                interface: common.interface.clone().into(),
                sync0_period: period,
                sync0_shift: shift,
                ..Default::default()
            };
            let opened = tokio::task::spawn_blocking(move || SoemLink::open(opt))
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

    let max_inflight = match common.mode {
        Mode::StopAndWait => 1,
        Mode::Streaming => common.inflight.max(1),
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
                match Box::pin(checker.check()).await {
                    Ok(status) => acc.observe(&status, start.elapsed()),
                    Err(_) => break,
                }
                tokio::time::sleep(poll).await;
            }
            acc
        })
    };

    let load = run_load(&client, common, max_inflight, start, total, shutdown).await;

    let acc = monitor.await.expect("monitor task panicked");
    let _ = client.close().await;

    let mut result = acc.into_result(CandidateResult::new(period, shift, cand.shift_percent));
    let (send_success, send_errors) = load?;
    result.send_success = send_success;
    result.send_errors = send_errors;
    Ok(result)
}

fn client_config(common: &Common, max_inflight: usize) -> ClientConfig {
    ClientConfig {
        timeout_cycles: common.timeout_cycles,
        max_inflight: NonZeroUsize::new(max_inflight).unwrap(),
        send_interval_cycles: common.send_interval_cycles,
        max_resync_rounds: common.max_resync_rounds,
        low_latency: common.low_latency,
        reset_resend_cycles: 2,
        rt_priority: common.rt_priority.map(|p| {
            ThreadPriority::Crossplatform(
                ThreadPriorityValue::try_from(p).expect("validated to 0..=99"),
            )
        }),
        rt_affinity: common.rt_core.map(|id| CoreId { id }),
    }
}

async fn run_load(
    client: &Client,
    common: &Common,
    max_inflight: usize,
    start: Instant,
    total: Duration,
    shutdown: &Arc<AtomicBool>,
) -> Result<(u64, u64)> {
    let xor_cmd = XorHashCmd {
        sleep_ms: common.sleep_ms,
        data: build_zero_xor_data(common.data_len),
    };
    let datagrams = client
        .datagram_builder()
        .push(&xor_cmd)
        .build()
        .context("building XorHash frame")?;

    match common.mode {
        Mode::StopAndWait => load_stop_and_wait(client, &datagrams, start, total, shutdown).await,
        Mode::Streaming => {
            load_streaming(client, &datagrams, start, total, shutdown, max_inflight).await
        }
    }
}

async fn load_stop_and_wait(
    client: &Client,
    datagrams: &Datagrams,
    start: Instant,
    total: Duration,
    shutdown: &Arc<AtomicBool>,
) -> Result<(u64, u64)> {
    let mut ok = 0u64;
    let mut err = 0u64;
    loop {
        if shutdown.load(Ordering::Relaxed) || start.elapsed() >= total {
            break;
        }
        match client
            .send_checked(datagrams.frame(0).expect("one frame"))
            .await
        {
            Ok(()) => ok += 1,
            Err(ClientError::InvalidPayload(msg)) => {
                anyhow::bail!("payload rejected by the local encoder: {msg}")
            }
            Err(_) => err += 1,
        }
    }
    Ok((ok, err))
}

async fn load_streaming(
    client: &Client,
    datagrams: &Datagrams,
    start: Instant,
    total: Duration,
    shutdown: &Arc<AtomicBool>,
    max_inflight: usize,
) -> Result<(u64, u64)> {
    let mut ok = 0u64;
    let mut err = 0u64;
    let mut pending: VecDeque<ResponseFuture> = VecDeque::with_capacity(max_inflight);
    loop {
        let stop = shutdown.load(Ordering::Relaxed) || start.elapsed() >= total;
        if !stop && pending.len() < max_inflight {
            match client.send(datagrams.frame(0).expect("one frame")).await {
                Ok(fut) => pending.push_back(fut),
                Err(ClientError::InvalidPayload(msg)) => {
                    anyhow::bail!("payload rejected by the local encoder: {msg}")
                }
                Err(_) => err += 1,
            }
            continue;
        }
        let Some(fut) = pending.pop_front() else {
            break;
        };
        match fut.await {
            Ok(_) => ok += 1,
            Err(ClientError::InvalidPayload(msg)) => {
                anyhow::bail!("payload rejected by the local encoder: {msg}")
            }
            Err(_) => err += 1,
        }
    }
    Ok((ok, err))
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
