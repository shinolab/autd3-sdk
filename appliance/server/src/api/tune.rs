use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_rs_appliance::{
    FRAME_PHASE_AUTO, TuneCandidate, TuneReport, TuneRequest, TuneStatus, TuneTarget,
    best_tune_candidate,
};
use autd3_rs_core::DeviceState;
use autd3_rs_link_echocat::{EchocatLinkOption, FramePhase};
use autd3_rs_link_remote::{Actual, Desired, Sessions, SharedBus};

const POLL_STEP: Duration = Duration::from_millis(50);
const MAX_CANDIDATES: usize = 256;
const MAX_SWEEP: Duration = Duration::from_hours(6);

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

    if !sleep_interruptibly(sweep, Duration::from_nanos(request.warmup_ns)) {
        return TuneCandidate {
            status: TuneStatus::Aborted,
            note: Some("cancelled during the warmup".to_owned()),
            ..candidate
        };
    }

    let base = sweep.bus.sampled();
    let poll = Duration::from_nanos(request.poll_ns);
    let dwell = Duration::from_nanos(request.dwell_ns);
    let started = Instant::now();
    let mut samples = 0u64;
    let mut op_samples = 0u64;
    let mut drop_events = 0u64;
    let mut previously_op = true;

    while started.elapsed() < dwell {
        if sweep.job.cancelled() {
            break;
        }
        let snapshot = sweep.bus.sampled();
        let all_op = matches!(snapshot.actual, Actual::Open)
            && !snapshot.devices.is_empty()
            && snapshot
                .devices
                .iter()
                .all(|state| *state == DeviceState::Op);
        samples += 1;
        if all_op {
            op_samples += 1;
        } else if previously_op {
            drop_events += 1;
        }
        previously_op = all_op;
        std::thread::sleep(poll);
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
        samples,
        op_samples,
        drop_events,
        recoveries: end.recoveries.saturating_sub(base.recoveries),
        stale_cycles: end.stale_cycles.saturating_sub(base.stale_cycles),
        lost_cycles: end.lost_cycles.saturating_sub(base.lost_cycles),
        phase_excursions: end.phase_excursions.saturating_sub(base.phase_excursions),
        exchanges: end.exchanges,
        exchange_mean_ns: end.exchange_mean_ns,
        exchange_worst_ns: end.exchange_worst_ns,
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
