use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_cpu_wire::{Cmd, Telemetry};
use autd3_rs_appliance::{
    FRAME_PHASE_AUTO, TuneCandidate, TuneReport, TuneRequest, TuneStatus, TuneTarget,
    best_tune_candidate,
};
use autd3_rs_core::DeviceState;
use autd3_rs_core::protocol::{RX_FRAME_BYTES, RxFrame, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs_link_echocat::master::budget::{WireTiming, exchange_budget};
use autd3_rs_link_echocat::{EchocatLinkOption, FramePhase, MAX_SYNC0_PERIOD};
use autd3_rs_link_remote::{Actual, Desired, RemoteLinkError, Sessions, SharedBus};

const POLL_STEP: Duration = Duration::from_millis(50);
const MAX_CANDIDATES: usize = 256;
const MAX_SWEEP: Duration = Duration::from_hours(6);
const MTU_BYTES: usize = 1500;
const TELEMETRY_TIMEOUT: Duration = Duration::from_millis(200);
const RESET_RESEND_CYCLES: usize = 4;

pub struct LinkSettings {
    base: EchocatLinkOption,
    current: Mutex<EchocatLinkOption>,
}

impl LinkSettings {
    pub fn new(option: EchocatLinkOption) -> Self {
        Self {
            current: Mutex::new(option.clone()),
            base: option,
        }
    }

    pub fn current(&self) -> EchocatLinkOption {
        self.lock().clone()
    }

    pub fn base(&self) -> &EchocatLinkOption {
        &self.base
    }

    fn set(&self, option: EchocatLinkOption) {
        *self.lock() = option;
    }

    fn restore(&self) {
        self.set(self.base.clone());
    }

    fn lock(&self) -> MutexGuard<'_, EchocatLinkOption> {
        self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Default)]
pub struct TuneJob {
    report: Mutex<TuneReport>,
    cancel: AtomicBool,
}

impl TuneJob {
    pub fn report(&self) -> TuneReport {
        self.lock().clone()
    }

    pub fn cancel(&self) -> bool {
        if !self.lock().running {
            return false;
        }
        self.cancel.store(true, Ordering::Release);
        true
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    fn lock(&self) -> MutexGuard<'_, TuneReport> {
        self.report.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn start(&self, total: usize) -> bool {
        let mut report = self.lock();
        if report.running {
            return false;
        }
        self.cancel.store(false, Ordering::Release);
        *report = TuneReport {
            running: true,
            total,
            ..TuneReport::default()
        };
        true
    }

    fn begin_candidate(&self, target: TuneTarget) {
        self.lock().current = Some(target);
    }

    fn finish_candidate(&self, candidate: TuneCandidate) {
        let mut report = self.lock();
        report.candidates.push(candidate);
        report.current = None;
    }

    fn stop_cancelling(&self) {
        self.cancel.store(false, Ordering::Release);
    }

    fn set_error(&self, message: String) {
        self.lock().error = Some(message);
    }

    fn finish(&self, cancelled: bool) {
        let mut report = self.lock();
        report.running = false;
        report.current = None;
        report.cancelled = cancelled;
        report.best = best_tune_candidate(&report.candidates);
    }
}

pub fn expand(request: &TuneRequest) -> Vec<TuneTarget> {
    let mut targets = Vec::new();
    for &period_ns in &request.periods_ns {
        for &frame_phase_percent in &request.frame_phase_percents {
            targets.push(TuneTarget {
                period_ns,
                frame_phase_percent,
                frame_phase_ns: period_ns.saturating_mul(u64::from(frame_phase_percent)) / 100,
            });
        }
    }
    targets
}

pub fn validate(request: &TuneRequest) -> Result<Vec<TuneTarget>, String> {
    if request.periods_ns.is_empty() || request.frame_phase_percents.is_empty() {
        return Err("a sweep needs at least one period and one frame phase".to_owned());
    }
    if request.periods_ns.contains(&0) {
        return Err("every period must be greater than zero".to_owned());
    }
    if let Some(period_ns) = request
        .periods_ns
        .iter()
        .find(|period_ns| u128::from(**period_ns) > MAX_SYNC0_PERIOD.as_nanos())
    {
        return Err(format!(
            "a period of {period_ns} ns is past the {MAX_SYNC0_PERIOD:?} the SYNC0 cycle time \
             register can hold",
        ));
    }
    if let Some(percent) = request
        .frame_phase_percents
        .iter()
        .find(|percent| **percent > 99)
    {
        return Err(format!(
            "a frame phase of {percent}% lands on the SYNC0 edge, where the firmware drops the \
             frame as a sequence mismatch; use 1..=99, or 0 to let the exchange centre it",
        ));
    }
    if request.dwell_ns == 0 || request.poll_ns == 0 {
        return Err("the dwell and the poll interval must be greater than zero".to_owned());
    }
    let targets = expand(request);
    if targets.len() > MAX_CANDIDATES {
        return Err(format!(
            "{} candidates is past the {MAX_CANDIDATES} this appliance sweeps in one run",
            targets.len(),
        ));
    }
    let per_candidate = Duration::from_nanos(request.warmup_ns)
        + Duration::from_nanos(request.dwell_ns)
        + Duration::from_nanos(request.settle_ns);
    if per_candidate.saturating_mul(u32::try_from(targets.len()).unwrap_or(u32::MAX)) > MAX_SWEEP {
        return Err(format!(
            "{} candidates at {per_candidate:?} each would run past the {MAX_SWEEP:?} cap",
            targets.len(),
        ));
    }
    Ok(targets)
}

pub struct Sweep {
    pub job: Arc<TuneJob>,
    pub bus: Arc<SharedBus>,
    pub sessions: Arc<Sessions>,
    pub settings: Arc<LinkSettings>,
    pub timing: WireTiming,
}

pub fn spawn(sweep: Sweep, request: TuneRequest, targets: Vec<TuneTarget>) -> Result<(), String> {
    if !sweep.job.start(targets.len()) {
        return Err("a sweep is already running".to_owned());
    }
    let job = Arc::clone(&sweep.job);
    match std::thread::Builder::new()
        .name("autd3-remote-tune".to_owned())
        .spawn(move || run(&sweep, &request, &targets))
    {
        Ok(_) => Ok(()),
        Err(e) => {
            job.finish(false);
            Err(format!("failed to spawn the sweep thread: {e}"))
        }
    }
}

fn run(sweep: &Sweep, request: &TuneRequest, targets: &[TuneTarget]) {
    let _guard = SweepGuard::new(sweep, request);
    tracing::info!(
        candidates = targets.len(),
        "starting a SYNC0 period and frame phase sweep; the bus is taken over until it finishes",
    );

    if let Some(session) = sweep.sessions.current() {
        sweep.job.set_error(format!(
            "{} connected before the sweep took the bus; disconnect it and start again",
            session.peer,
        ));
        return;
    }

    for target in targets {
        if sweep.job.cancelled() {
            break;
        }
        sweep.job.begin_candidate(*target);
        sweep.job.finish_candidate(measure(sweep, request, *target));
    }
}

struct SweepGuard<'a> {
    sweep: &'a Sweep,
    settle: Duration,
    desired: Desired,
}

impl<'a> SweepGuard<'a> {
    fn new(sweep: &'a Sweep, request: &TuneRequest) -> Self {
        let desired = sweep.bus.snapshot().desired;
        sweep
            .bus
            .hold("a tune sweep is driving the bus; wait for it to finish");
        Self {
            sweep,
            settle: Duration::from_nanos(request.settle_ns),
            desired,
        }
    }
}

impl Drop for SweepGuard<'_> {
    fn drop(&mut self) {
        let sweep = self.sweep;
        let cancelled = sweep.job.cancelled() && !std::thread::panicking();
        sweep.settings.restore();
        sweep.job.stop_cancelling();
        close(sweep, self.settle);
        sweep.bus.set_desired(self.desired);
        sweep.bus.release();
        if std::thread::panicking() {
            sweep.job.set_error(
                "the sweep thread panicked; the configured bus settings are back".to_owned(),
            );
        }
        sweep.job.finish(cancelled);
        tracing::info!(
            cancelled,
            "the sweep finished and the configured bus settings are back",
        );
    }
}

fn option_for(base: &EchocatLinkOption, target: TuneTarget) -> EchocatLinkOption {
    EchocatLinkOption {
        sync0_period: Duration::from_nanos(target.period_ns),
        frame_phase: if target.frame_phase_percent == FRAME_PHASE_AUTO {
            FramePhase::Auto
        } else {
            FramePhase::At(Duration::from_nanos(target.frame_phase_ns))
        },
        ..base.clone()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Counters {
    processed: u64,
    seq_mismatch: u64,
}

#[derive(Clone, Copy, Debug)]
struct DeviceTelemetry {
    processed: u8,
    seq_mismatch: u8,
}

struct Driver {
    seq: u8,
    tx: Vec<[u8; TX_FRAME_BYTES]>,
    rx: Vec<[u8; RX_FRAME_BYTES]>,
}

impl Driver {
    fn new(devices: usize) -> Self {
        Self {
            seq: 0,
            tx: vec![[0u8; TX_FRAME_BYTES]; devices],
            rx: vec![[0u8; RX_FRAME_BYTES]; devices],
        }
    }

    fn write(&mut self, cmd: Cmd, payload: impl Fn(&mut [u8])) -> Seq {
        self.seq = self.seq.wrapping_add(1);
        let seq = Seq::new(self.seq);
        let mut frame = TxFrame::new(seq, cmd);
        payload(&mut frame.payload);
        for slot in &mut self.tx {
            frame.write_to(slot);
        }
        seq
    }

    fn handshake(&mut self, sweep: &Sweep) -> bool {
        let frame = TxFrame::new(Seq::ZERO, Cmd::Reset);
        for slot in &mut self.tx {
            frame.write_to(slot);
        }
        for _ in 0..RESET_RESEND_CYCLES {
            if let Err(e) = sweep.bus.exchange_while_held(&self.tx, &mut self.rx) {
                tracing::warn!(error = %e, "could not reset the devices for the sweep");
                return false;
            }
        }
        self.seq = u8::MAX;
        true
    }

    fn drive(&mut self, sweep: &Sweep) -> Result<(), RemoteLinkError> {
        self.write(Cmd::Nop, |_| {});
        sweep
            .bus
            .exchange_while_held(&self.tx, &mut self.rx)
            .map(|_| ())
    }

    fn acknowledged(&self, seq: Seq) -> bool {
        self.rx.iter().all(|slot| RxFrame::parse(slot).ack == seq)
    }

    fn read_counter(&mut self, sweep: &Sweep, counter: Telemetry) -> Option<Vec<u8>> {
        let seq = self.write(Cmd::ReadTelemetry, |payload| {
            payload[0] = counter.as_u8();
        });
        let deadline = Instant::now() + TELEMETRY_TIMEOUT;
        loop {
            if let Err(e) = sweep.bus.exchange_while_held(&self.tx, &mut self.rx) {
                tracing::warn!(error = %e, ?counter, "could not drive a telemetry read");
                return None;
            }
            if self.acknowledged(seq) {
                return Some(
                    self.rx
                        .iter()
                        .map(|slot| RxFrame::parse(slot).data)
                        .collect(),
                );
            }
            if Instant::now() >= deadline {
                tracing::warn!(
                    ?counter,
                    timeout = ?TELEMETRY_TIMEOUT,
                    "no device acknowledged the telemetry read",
                );
                return None;
            }
        }
    }

    fn read_telemetry(&mut self, sweep: &Sweep) -> Option<Vec<DeviceTelemetry>> {
        let processed = self.read_counter(sweep, Telemetry::Processed)?;
        let seq_mismatch = self.read_counter(sweep, Telemetry::SeqMismatch)?;
        Some(
            processed
                .into_iter()
                .zip(seq_mismatch)
                .map(|(processed, seq_mismatch)| DeviceTelemetry {
                    processed,
                    seq_mismatch,
                })
                .collect(),
        )
    }
}

#[derive(Clone, Debug, Default)]
struct Observed {
    samples: u64,
    op_samples: u64,
    drop_events: u64,
    counters: Counters,
    telemetry_read: bool,
    error: Option<String>,
}

fn observe(sweep: &Sweep, request: &TuneRequest, driver: &mut Driver) -> Observed {
    let poll = Duration::from_nanos(request.poll_ns);
    let dwell = Duration::from_nanos(request.dwell_ns);
    let started = Instant::now();
    let mut next_sample = started;
    let mut seen = Observed::default();
    let mut previously_op = true;
    let mut last = driver.read_telemetry(sweep);
    seen.telemetry_read = last.is_some();
    let mut totals = vec![Counters::default(); last.as_ref().map_or(0, Vec::len)];

    while started.elapsed() < dwell {
        if sweep.job.cancelled() {
            break;
        }
        if let Err(e) = driver.drive(sweep) {
            tracing::warn!(error = %e, "the bus failed under the sweep's frames");
            seen.error = Some(format!("the bus failed during the dwell: {e}"));
            break;
        }
        if Instant::now() < next_sample {
            continue;
        }
        next_sample += poll;
        if let Some(before) = last.take() {
            match driver.read_telemetry(sweep) {
                Some(after) => {
                    for ((total, before), after) in totals.iter_mut().zip(&before).zip(&after) {
                        total.processed +=
                            u64::from(after.processed.wrapping_sub(before.processed));
                        total.seq_mismatch +=
                            u64::from(after.seq_mismatch.wrapping_sub(before.seq_mismatch));
                    }
                    last = Some(after);
                }
                None => seen.telemetry_read = false,
            }
        }
        let snapshot = sweep.bus.sampled();
        let all_op = matches!(snapshot.actual, Actual::Open)
            && !snapshot.devices.is_empty()
            && snapshot
                .devices
                .iter()
                .all(|state| *state == DeviceState::Op);
        seen.samples += 1;
        if all_op {
            seen.op_samples += 1;
        } else if previously_op {
            seen.drop_events += 1;
        }
        previously_op = all_op;
    }
    seen.counters = Counters {
        processed: totals.iter().map(|t| t.processed).min().unwrap_or(0),
        seq_mismatch: totals.iter().map(|t| t.seq_mismatch).max().unwrap_or(0),
    };
    seen
}

fn infeasible_note(devices: usize, budget: Duration, target: TuneTarget) -> Option<String> {
    let period = Duration::from_nanos(target.period_ns);
    let us = |d: Duration| d.as_micros();
    if budget >= period {
        return Some(format!(
            "{devices} devices need about {} us on the wire, which does not fit in a {} us \
             period; one exchange cannot finish before the next SYNC0",
            us(budget),
            us(period),
        ));
    }
    if target.is_auto() {
        return None;
    }
    let landing = Duration::from_nanos(target.frame_phase_ns);
    (landing + budget > period).then(|| {
        format!(
            "a landing at {} us leaves {} us before the next SYNC0, but {devices} devices need \
             about {} us on the wire; the exchange would run past the edge",
            us(landing),
            us(period.saturating_sub(landing)),
            us(budget),
        )
    })
}

fn measure(sweep: &Sweep, request: &TuneRequest, target: TuneTarget) -> TuneCandidate {
    let candidate = TuneCandidate {
        target,
        ..TuneCandidate::default()
    };

    sweep
        .settings
        .set(option_for(sweep.settings.base(), target));
    let settle = Duration::from_nanos(request.settle_ns);
    if !close(sweep, settle) {
        return TuneCandidate {
            status: TuneStatus::Aborted,
            note: Some("the bus never went back to closed".to_owned()),
            ..candidate
        };
    }

    sweep.bus.set_desired(Desired::Open);
    if !wait_until(sweep, settle, |actual| matches!(actual, Actual::Open)) {
        let snapshot = sweep.bus.snapshot();
        return TuneCandidate {
            status: TuneStatus::FailedOpen,
            note: Some(match &snapshot.actual {
                Actual::Failed { reason } => reason.clone(),
                other => format!("the bus stayed {other:?} for {settle:?}"),
            }),
            ..candidate
        };
    }

    let devices = sweep.bus.snapshot().num_devices;
    let budget = exchange_budget(devices, MTU_BYTES, sweep.timing);
    if let Some(note) = infeasible_note(devices, budget, target) {
        return TuneCandidate {
            status: TuneStatus::Infeasible,
            note: Some(note),
            num_devices: devices,
            ..candidate
        };
    }

    if !sleep_interruptibly(sweep, Duration::from_nanos(request.warmup_ns)) {
        return TuneCandidate {
            status: TuneStatus::Aborted,
            note: Some("cancelled during the warmup".to_owned()),
            ..candidate
        };
    }

    let mut driver = Driver::new(devices);
    if !driver.handshake(sweep) {
        return TuneCandidate {
            status: TuneStatus::Aborted,
            note: Some("the devices never accepted the sweep's reset".to_owned()),
            num_devices: devices,
            ..candidate
        };
    }

    let base = sweep.bus.sampled();
    let observed = observe(sweep, request, &mut driver);
    if let Some(note) = observed.error {
        return TuneCandidate {
            status: TuneStatus::Aborted,
            note: Some(note),
            num_devices: devices,
            ..candidate
        };
    }

    let end = sweep.bus.sampled();
    let cancelled = sweep.job.cancelled();
    TuneCandidate {
        status: if cancelled {
            TuneStatus::Aborted
        } else {
            TuneStatus::Ok
        },
        note: cancelled.then(|| "cancelled during the dwell".to_owned()),
        num_devices: end.num_devices,
        samples: observed.samples,
        op_samples: observed.op_samples,
        drop_events: observed.drop_events,
        recoveries: end.recoveries.saturating_sub(base.recoveries),
        stale_cycles: end.stale_cycles.saturating_sub(base.stale_cycles),
        lost_cycles: end.lost_cycles.saturating_sub(base.lost_cycles),
        phase_excursions: end.phase_excursions.saturating_sub(base.phase_excursions),
        exchanges: end.exchanges,
        exchange_mean_ns: end.exchange_mean_ns,
        exchange_worst_ns: end.exchange_worst_ns,
        frames_delivered: observed.counters.processed,
        frames_skipped: observed.counters.seq_mismatch,
        telemetry_read: observed.telemetry_read,
        ..candidate
    }
}

fn close(sweep: &Sweep, settle: Duration) -> bool {
    sweep.bus.set_desired(Desired::Closed);
    wait_until(sweep, settle, |actual| {
        matches!(actual, Actual::Closed | Actual::Failed { .. })
    })
}

fn wait_until(sweep: &Sweep, timeout: Duration, reached: impl Fn(&Actual) -> bool) -> bool {
    sweep
        .bus
        .wait_actual(timeout, reached, || sweep.job.cancelled())
}

fn sleep_interruptibly(sweep: &Sweep, total: Duration) -> bool {
    let deadline = Instant::now() + total;
    while Instant::now() < deadline {
        if sweep.job.cancelled() {
            return false;
        }
        std::thread::sleep(POLL_STEP.min(deadline.saturating_duration_since(Instant::now())));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> TuneRequest {
        TuneRequest {
            periods_ns: vec![1_000_000, 2_000_000],
            frame_phase_percents: vec![FRAME_PHASE_AUTO, 25],
            ..TuneRequest::default()
        }
    }

    #[test]
    fn every_period_is_paired_with_every_frame_phase() {
        let targets = expand(&request());
        assert_eq!(targets.len(), 4);
        assert_eq!(
            targets[3],
            TuneTarget {
                period_ns: 2_000_000,
                frame_phase_percent: 25,
                frame_phase_ns: 500_000,
            },
        );
    }

    #[test]
    fn an_automatic_frame_phase_carries_no_resolved_value() {
        let targets = expand(&request());
        assert!(targets[0].is_auto());
        assert_eq!(targets[0].frame_phase_ns, 0);
    }

    #[test]
    fn a_frame_phase_on_the_sync0_edge_is_refused() {
        let err = validate(&TuneRequest {
            frame_phase_percents: vec![100],
            ..TuneRequest::default()
        })
        .unwrap_err();
        assert!(err.contains("SYNC0 edge"), "{err}");
    }

    #[test]
    fn a_sweep_that_would_run_for_days_is_refused_before_it_takes_the_bus() {
        let err = validate(&TuneRequest {
            periods_ns: (1..=40).map(|n| n * 100_000).collect(),
            frame_phase_percents: (1..=40).collect(),
            ..TuneRequest::default()
        })
        .unwrap_err();
        assert!(err.contains("candidates"), "{err}");
    }

    #[test]
    fn a_period_the_sync0_register_cannot_hold_is_refused_before_it_reaches_the_bus() {
        let err = validate(&TuneRequest {
            periods_ns: vec![5_000_000_000],
            ..TuneRequest::default()
        })
        .unwrap_err();
        assert!(err.contains("5000000000 ns"), "{err}");
        assert!(
            validate(&TuneRequest {
                periods_ns: vec![u64::try_from(MAX_SYNC0_PERIOD.as_nanos()).unwrap()],
                ..TuneRequest::default()
            })
            .is_ok(),
        );
    }

    #[test]
    fn every_accepted_candidate_option_passes_the_link_validation() {
        let request = TuneRequest {
            periods_ns: vec![
                1,
                1_000_000,
                u64::try_from(MAX_SYNC0_PERIOD.as_nanos()).unwrap(),
            ],
            frame_phase_percents: vec![FRAME_PHASE_AUTO, 1, 99],
            ..TuneRequest::default()
        };
        let base = EchocatLinkOption::default();
        for target in validate(&request).unwrap() {
            assert!(
                option_for(&base, target).validate().is_ok(),
                "{target:?} was accepted by the sweep but rejected by the link",
            );
        }
    }

    #[test]
    fn an_empty_sweep_is_refused() {
        assert!(
            validate(&TuneRequest {
                periods_ns: vec![],
                ..TuneRequest::default()
            })
            .is_err()
        );
    }

    #[test]
    fn a_candidate_option_keeps_everything_the_config_chose() {
        let base = EchocatLinkOption {
            pdu_timeout: Duration::from_millis(42),
            ..EchocatLinkOption::default()
        };
        let option = option_for(
            &base,
            TuneTarget {
                period_ns: 2_000_000,
                frame_phase_percent: 25,
                frame_phase_ns: 500_000,
            },
        );
        assert_eq!(option.sync0_period, Duration::from_millis(2));
        assert_eq!(
            option.frame_phase,
            FramePhase::At(Duration::from_micros(500))
        );
        assert_eq!(option.pdu_timeout, Duration::from_millis(42));
    }

    #[test]
    fn a_frame_phase_on_a_ridiculous_period_saturates_instead_of_overflowing() {
        let targets = expand(&TuneRequest {
            periods_ns: vec![u64::MAX],
            frame_phase_percents: vec![99],
            ..TuneRequest::default()
        });
        assert_eq!(targets[0].frame_phase_ns, u64::MAX / 100);
    }

    fn budget_20() -> Duration {
        exchange_budget(20, MTU_BYTES, WireTiming::default())
    }

    fn target(micros: u64, percent: u8) -> TuneTarget {
        let period_ns = micros * 1_000;
        TuneTarget {
            period_ns,
            frame_phase_percent: percent,
            frame_phase_ns: period_ns * u64::from(percent) / 100,
        }
    }

    #[test]
    fn a_period_that_cannot_carry_one_exchange_is_refused_at_every_phase() {
        for percent in [FRAME_PHASE_AUTO, 1, 25, 50, 75, 99] {
            let note = infeasible_note(20, budget_20(), target(1_000, percent))
                .unwrap_or_else(|| panic!("1 ms / 20 devices was accepted at {percent}%"));
            assert!(note.contains("does not fit"), "{note}");
        }
    }

    #[test]
    fn a_landing_that_runs_the_exchange_past_the_sync0_edge_is_refused() {
        for percent in [FRAME_PHASE_AUTO, 1, 25] {
            assert!(
                infeasible_note(20, budget_20(), target(2_000, percent)).is_none(),
                "2 ms at {percent}% leaves room for the exchange",
            );
        }
        for percent in [50, 75, 99] {
            let note = infeasible_note(20, budget_20(), target(2_000, percent))
                .unwrap_or_else(|| panic!("2 ms at {percent}% was accepted"));
            assert!(note.contains("past the edge"), "{note}");
        }
    }

    #[test]
    fn the_measured_sweep_no_longer_recommends_the_worst_candidate() {
        let rows: [(u64, u8, u64, u64, u64, u64); 12] = [
            (1_000, FRAME_PHASE_AUTO, 15, 25_069, 27_200, 1_133_484),
            (1_000, 1, 13, 26_446, 27_200, 1_126_969),
            (1_000, 25, 12, 20_952, 27_200, 1_146_119),
            (1_000, 50, 9, 13_221, 27_200, 1_142_083),
            (1_000, 75, 4, 11, 27_200, 1_145_212),
            (1_000, 99, 3, 0, 27_200, 1_149_268),
            (2_000, FRAME_PHASE_AUTO, 1, 5, 15_000, 2_122_814),
            (2_000, 1, 0, 108, 15_000, 1_140_273),
            (2_000, 25, 0, 0, 15_000, 1_138_942),
            (2_000, 50, 1, 0, 15_000, 2_114_296),
            (2_000, 75, 0, 0, 15_000, 1_139_649),
            (2_000, 99, 0, 0, 15_000, 1_132_447),
        ];
        let candidates: Vec<TuneCandidate> = rows
            .iter()
            .map(
                |&(period_us, percent, stale, excursions, exchanges, worst)| {
                    let target = target(period_us, percent);
                    let status = if infeasible_note(20, budget_20(), target).is_some() {
                        TuneStatus::Infeasible
                    } else {
                        TuneStatus::Ok
                    };
                    TuneCandidate {
                        target,
                        status,
                        num_devices: 20,
                        samples: if status == TuneStatus::Ok { 300 } else { 0 },
                        op_samples: if status == TuneStatus::Ok { 300 } else { 0 },
                        stale_cycles: stale,
                        phase_excursions: excursions,
                        exchanges,
                        exchange_worst_ns: worst,
                        exchange_mean_ns: 1_103_700,
                        ..TuneCandidate::default()
                    }
                },
            )
            .collect();

        let feasible: Vec<&TuneCandidate> = candidates
            .iter()
            .filter(|c| c.status == TuneStatus::Ok)
            .collect();
        assert_eq!(
            feasible.len(),
            3,
            "only 2 ms at auto / 1% / 25% leaves room for a 1059 us exchange",
        );

        let best = best_tune_candidate(&candidates).expect("a feasible candidate remains");
        assert_eq!(
            candidates[best].target,
            target(2_000, 25),
            "the recommendation is the candidate that held its landing phase, not the one with \
             the smallest worst exchange",
        );
        assert_ne!(
            candidates[best].target,
            target(1_000, 1),
            "the old score picked the row with the most phase excursions in the whole table",
        );
    }

    #[test]
    fn an_error_recorded_before_the_finish_survives_it() {
        let job = TuneJob::default();
        assert!(job.start(1));
        job.set_error("a client connected".to_owned());
        job.finish(false);
        let report = job.report();
        assert!(!report.running);
        assert_eq!(report.error.as_deref(), Some("a client connected"));
    }

    #[test]
    fn a_job_only_runs_one_sweep_at_a_time() {
        let job = TuneJob::default();
        assert!(job.start(2));
        assert!(!job.start(2));
        job.finish(false);
        assert!(job.start(2));
    }

    #[test]
    fn cancelling_an_idle_job_is_not_an_error_it_is_simply_nothing() {
        let job = TuneJob::default();
        assert!(!job.cancel());
        assert!(job.start(1));
        assert!(job.cancel());
        assert!(job.cancelled());
    }

    #[test]
    fn a_cancelled_sweep_still_waits_for_the_bus_while_it_puts_the_settings_back() {
        let job = TuneJob::default();
        assert!(job.start(1));
        assert!(job.cancel());
        job.stop_cancelling();
        assert!(
            !job.cancelled(),
            "the restore path has to wait for the close, or the bus keeps running the last \
             candidate's period",
        );
        job.finish(true);
        assert!(job.report().cancelled, "the report still has to say so");
    }
}
