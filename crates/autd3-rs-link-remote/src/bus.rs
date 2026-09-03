use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use autd3_rs_core::link::{DcClock, DeviceState, Link, LinkStats, LinkStatus, StateCheck};
use autd3_rs_core::value::DcSysTime;
use autd3_rs_core::{
    CoreId, RX_FRAME_BYTES, RtPriority, RtSchedulePolicy, RtThreadTuning, TX_FRAME_BYTES,
};

use crate::error::RemoteLinkError;
use crate::wire::{self, BusStatus};

const DEFAULT_CYCLE_PERIOD: Duration = Duration::from_micros(250);
const REOPEN_RETRY_PERIOD: Duration = Duration::from_secs(1);
const RECOVERING_REPLY_PERIOD: Duration = Duration::from_millis(1);
const STATUS_SAMPLE_PERIOD: Duration = Duration::from_millis(100);
const IDLE_WAIT_PERIOD: Duration = Duration::from_millis(100);
const SHUTDOWN_POLL_PERIOD: Duration = Duration::from_millis(20);
const OPEN_WAIT_PERIOD: Duration = Duration::from_millis(10);
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const TUNING_WAIT_TIMEOUT: Duration = Duration::from_secs(1);
const STACK_HEADROOM_BYTES: usize = 1024 * 1024;
const PREFAULT_FRAME_BYTES: usize = 16 * 1024;
const PAGE_BYTES: usize = 4096;
const DIAG_REPORT_PERIOD: Duration = Duration::from_secs(5);
const DIAG_LOCK_ALERT: Duration = Duration::from_micros(100);
const DIAG_SLOW_ITERATION: Duration = Duration::from_millis(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BusPacing {
    LinkPaced,
    Period(Duration),
}

impl Default for BusPacing {
    fn default() -> Self {
        Self::Period(DEFAULT_CYCLE_PERIOD)
    }
}

impl BusPacing {
    fn wait_after(self, start: Instant) {
        let Self::Period(period) = self else {
            return;
        };
        let remaining = period.saturating_sub(start.elapsed());
        if !remaining.is_zero() {
            std::thread::sleep(remaining);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BusOption {
    pub pacing: BusPacing,
    pub rt_priority: Option<RtPriority>,
    pub rt_policy: RtSchedulePolicy,
    pub rt_affinity: Option<CoreId>,
    pub stack_prefault_bytes: usize,
}

impl Default for BusOption {
    fn default() -> Self {
        Self {
            pacing: BusPacing::default(),
            rt_priority: autd3_rs_core::default_rt_priority(),
            rt_policy: RtSchedulePolicy::default(),
            rt_affinity: None,
            stack_prefault_bytes: 0,
        }
    }
}

impl BusOption {
    fn tuning(&self) -> RtThreadTuning {
        RtThreadTuning {
            priority: self.rt_priority,
            policy: self.rt_policy,
            affinity: self.rt_affinity,
        }
    }

    pub(crate) fn session_tuning(&self) -> RtThreadTuning {
        RtThreadTuning {
            priority: self.rt_priority.and_then(RtPriority::step_below),
            policy: self.rt_policy,
            affinity: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Desired {
    #[default]
    Closed,
    Open,
}

impl Desired {
    fn from_bits(bits: u8) -> Self {
        if bits == 0 { Self::Closed } else { Self::Open }
    }

    fn bits(self) -> u8 {
        match self {
            Self::Closed => 0,
            Self::Open => 1,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Actual {
    #[default]
    Closed,
    Opening,
    Open,
    Recovering,
    Failed {
        reason: String,
    },
}

impl std::fmt::Display for Actual {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => f.write_str("closed"),
            Self::Opening => f.write_str("opening"),
            Self::Open => f.write_str("open"),
            Self::Recovering => f.write_str("recovering"),
            Self::Failed { reason } => write!(f, "failed: {reason}"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BusSnapshot {
    pub desired: Desired,
    pub actual: Actual,
    pub stopped: bool,
    pub num_devices: usize,
    pub devices: Vec<DeviceState>,
    pub recoveries: u64,
    pub stale_cycles: u64,
    pub lost_cycles: u64,
    pub phase_excursions: u64,
    pub worst_phase_deviation_ns: u64,
    pub exchanges: u64,
    pub exchange_mean_ns: u64,
    pub exchange_worst_ns: u64,
}

enum Probe {
    Idle,
    Requested,
    Running,
    Done(Result<usize, String>),
}

#[derive(Default)]
struct CounterBase {
    recoveries: u64,
    stale_cycles: u64,
    lost_cycles: u64,
    phase_excursions: u64,
    worst_phase_deviation_ns: u64,
}

struct BusState {
    actual: Actual,
    num_devices: usize,
    tx: Vec<[u8; TX_FRAME_BYTES]>,
    rx: Vec<[u8; RX_FRAME_BYTES]>,
    rx_valid: bool,
    dc_time_ns: u64,
    tx_version: u64,
    applied_version: u64,
    status: BusStatus,
    base: CounterBase,
    link_recoveries: u64,
    reopen_pending: bool,
    probe: Probe,
    applied_tuning: Option<RtThreadTuning>,
    hold: Option<String>,
}

impl BusState {
    fn fold_counters(&mut self) {
        self.base.recoveries = self.status.recoveries;
        self.base.stale_cycles = self.status.stale_cycles;
        self.base.lost_cycles = self.status.lost_cycles;
        self.base.phase_excursions = self.status.phase_excursions;
        self.base.worst_phase_deviation_ns = self.status.worst_phase_deviation_ns;
        self.link_recoveries = 0;
    }
}

pub(crate) struct FrameReply {
    pub(crate) rx_valid: bool,
    pub(crate) dc_time_ns: u64,
}

pub(crate) struct BusShared {
    state: Mutex<BusState>,
    cv: Condvar,
    stopped: AtomicBool,
    desired: AtomicU8,
    stats: Mutex<LinkStats>,
    sampled: Mutex<Arc<BusSnapshot>>,
}

impl BusShared {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(BusState {
                actual: Actual::Closed,
                num_devices: 0,
                tx: Vec::new(),
                rx: Vec::new(),
                rx_valid: false,
                dc_time_ns: wire::DC_TIME_UNAVAILABLE,
                tx_version: 0,
                applied_version: 0,
                status: BusStatus::default(),
                base: CounterBase::default(),
                link_recoveries: 0,
                reopen_pending: false,
                probe: Probe::Idle,
                applied_tuning: None,
                hold: None,
            }),
            cv: Condvar::new(),
            stopped: AtomicBool::new(false),
            desired: AtomicU8::new(Desired::Closed.bits()),
            stats: Mutex::new(LinkStats::default()),
            sampled: Mutex::new(Arc::new(BusSnapshot::default())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, BusState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn set_desired(&self, desired: Desired) {
        let _guard = self.lock();
        if self.desired.swap(desired.bits(), Ordering::Release) == desired.bits() {
            return;
        }
        self.cv.notify_all();
    }

    pub(crate) fn desired(&self) -> Desired {
        Desired::from_bits(self.desired.load(Ordering::Acquire))
    }

    pub(crate) fn stop(&self) {
        let _guard = self.lock();
        self.stopped.store(true, Ordering::Release);
        self.cv.notify_all();
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    pub(crate) fn snapshot(&self) -> BusSnapshot {
        let counters = self.stats();
        let state = self.lock();
        BusSnapshot {
            desired: self.desired(),
            actual: state.actual.clone(),
            stopped: self.is_stopped(),
            num_devices: state.num_devices,
            devices: state.status.devices.clone(),
            recoveries: state.status.recoveries,
            stale_cycles: state.status.stale_cycles,
            lost_cycles: state.status.lost_cycles,
            phase_excursions: state.status.phase_excursions,
            worst_phase_deviation_ns: state.status.worst_phase_deviation_ns,
            exchanges: counters.exchanges(),
            exchange_mean_ns: counters.mean_exchange_ns(),
            exchange_worst_ns: counters.worst_exchange_ns(),
        }
    }

    fn sample(&self) {
        let snapshot = Arc::new(self.snapshot());
        *self
            .sampled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = snapshot;
    }

    pub(crate) fn sampled(&self) -> Arc<BusSnapshot> {
        Arc::clone(
            &self
                .sampled
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    fn stats(&self) -> LinkStats {
        self.stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn set_stats(&self, stats: &LinkStats) {
        *self
            .stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = stats.clone();
    }

    pub(crate) fn set_hold(&self, reason: Option<String>) {
        let mut state = self.lock();
        state.hold = reason;
        self.cv.notify_all();
    }

    pub(crate) fn hold_reason(&self) -> Option<String> {
        self.lock().hold.clone()
    }

    pub(crate) fn wait_actual(
        &self,
        timeout: Duration,
        reached: impl Fn(&Actual) -> bool,
        give_up: impl Fn() -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.lock();
        loop {
            if reached(&state.actual) {
                return true;
            }
            if self.is_stopped() || give_up() {
                return false;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            state = self
                .cv
                .wait_timeout(state, remaining.min(OPEN_WAIT_PERIOD))
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    pub(crate) fn wait_for_open(&self, timeout: Duration) -> Result<usize, RemoteLinkError> {
        let deadline = Instant::now() + timeout;
        let mut state = self.lock();
        loop {
            if self.is_stopped() {
                return Err(RemoteLinkError::Link("the bus loop stopped".to_owned()));
            }
            if self.desired() == Desired::Closed {
                return Err(RemoteLinkError::BusClosed);
            }
            if state.actual == Actual::Open {
                return Ok(state.num_devices);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(RemoteLinkError::BusUnavailable {
                    reason: match &state.actual {
                        Actual::Failed { reason } => reason.clone(),
                        actual => format!("the bus is still {actual}"),
                    },
                });
            }
            state = self
                .cv
                .wait_timeout(state, remaining.min(OPEN_WAIT_PERIOD))
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    pub(crate) fn request_probe(&self) -> Result<usize, RemoteLinkError> {
        let mut state = self.lock();
        if self.is_stopped() {
            return Err(RemoteLinkError::Link("the bus loop stopped".to_owned()));
        }
        if self.desired() == Desired::Open {
            if state.actual == Actual::Open {
                return Ok(state.num_devices);
            }
            return Err(RemoteLinkError::Link(
                "the bus must be closed, or already open, to report a device count".to_owned(),
            ));
        }
        if !matches!(state.probe, Probe::Idle) {
            return Err(RemoteLinkError::Link(
                "a probe is already running".to_owned(),
            ));
        }
        state.probe = Probe::Requested;
        self.cv.notify_all();
        let deadline = Instant::now() + PROBE_TIMEOUT;
        loop {
            if matches!(state.probe, Probe::Done(_)) {
                let Probe::Done(result) = std::mem::replace(&mut state.probe, Probe::Idle) else {
                    unreachable!("the probe was just observed as done")
                };
                return result.map_err(|reason| RemoteLinkError::BusUnavailable { reason });
            }
            if self.is_stopped() {
                state.probe = Probe::Idle;
                return Err(RemoteLinkError::Link("the bus loop stopped".to_owned()));
            }
            if matches!(state.probe, Probe::Requested) && self.desired() == Desired::Open {
                state.probe = Probe::Idle;
                return Err(RemoteLinkError::ProbeBusOpened);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                state.probe = Probe::Idle;
                return Err(RemoteLinkError::ProbeTimeout {
                    timeout: PROBE_TIMEOUT,
                });
            }
            state = self
                .cv
                .wait_timeout(state, remaining.min(IDLE_WAIT_PERIOD))
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    fn publish_tuning(&self, tuning: RtThreadTuning) {
        let mut state = self.lock();
        state.applied_tuning = Some(tuning);
        self.cv.notify_all();
    }

    fn wait_for_tuning(&self, timeout: Duration) -> Option<RtThreadTuning> {
        let deadline = Instant::now() + timeout;
        let mut state = self.lock();
        loop {
            if let Some(tuning) = state.applied_tuning {
                return Some(tuning);
            }
            if self.is_stopped() {
                return None;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            state = self
                .cv
                .wait_timeout(state, remaining.min(OPEN_WAIT_PERIOD))
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    fn take_probe_request(&self) -> bool {
        let mut state = self.lock();
        if matches!(state.probe, Probe::Requested) {
            state.probe = Probe::Running;
            true
        } else {
            false
        }
    }

    fn finish_probe(&self, result: Result<usize, String>) {
        let mut state = self.lock();
        if matches!(state.probe, Probe::Running) {
            state.probe = Probe::Done(result);
        }
        self.cv.notify_all();
    }

    fn enter_closed(&self) {
        let mut state = self.lock();
        if state.actual == Actual::Closed {
            return;
        }
        state.actual = Actual::Closed;
        state.rx_valid = false;
        state.fold_counters();
        state.status.recovering = false;
        state.status.devices.clear();
        state.num_devices = 0;
        state.reopen_pending = false;
        self.cv.notify_all();
    }

    fn enter_opening(&self) {
        let mut state = self.lock();
        state.actual = Actual::Opening;
        state.rx_valid = false;
        self.cv.notify_all();
    }

    fn enter_open(&self, num_devices: usize) {
        let mut state = self.lock();
        state.actual = Actual::Open;
        if state.num_devices != num_devices {
            state.num_devices = num_devices;
            state.tx = vec![[0u8; TX_FRAME_BYTES]; num_devices];
            state.rx = vec![[0u8; RX_FRAME_BYTES]; num_devices];
            state.tx_version = 0;
            state.applied_version = 0;
        }
        state.status.devices = vec![DeviceState::Op; num_devices];
        state.status.recovering = false;
        if std::mem::take(&mut state.reopen_pending) {
            state.base.recoveries += 1;
        }
        state.status.recoveries = state.base.recoveries + state.link_recoveries;
        self.cv.notify_all();
    }

    fn enter_recovering(&self) {
        let mut state = self.lock();
        state.actual = Actual::Recovering;
        state.rx_valid = false;
        state.fold_counters();
        state.status.recovering = true;
        state.reopen_pending = true;
        self.cv.notify_all();
    }

    fn enter_failed(&self, reason: &str) {
        let mut state = self.lock();
        state.actual = Actual::Failed {
            reason: reason.to_owned(),
        };
        state.rx_valid = false;
        state.status.recovering = true;
        self.cv.notify_all();
    }

    fn take_tx(&self, tx: &mut [[u8; TX_FRAME_BYTES]]) -> (u64, Duration) {
        let waiting = Instant::now();
        let state = self.lock();
        let waited = waiting.elapsed();
        tx.copy_from_slice(&state.tx);
        (state.tx_version, waited)
    }

    fn publish_cycle(
        &self,
        rx: &[[u8; RX_FRAME_BYTES]],
        rx_valid: bool,
        dc_time_ns: u64,
        version: u64,
        counters: &LinkStats,
    ) -> Duration {
        let waiting = Instant::now();
        let mut state = self.lock();
        let waited = waiting.elapsed();
        state.rx.copy_from_slice(rx);
        state.rx_valid = rx_valid;
        state.dc_time_ns = dc_time_ns;
        state.applied_version = version;
        state.status.stale_cycles = state.base.stale_cycles + counters.stale_cycles();
        state.status.lost_cycles = state.base.lost_cycles + counters.lost_cycles();
        state.status.phase_excursions = state.base.phase_excursions + counters.phase_excursions();
        state.status.worst_phase_deviation_ns = state
            .base
            .worst_phase_deviation_ns
            .max(counters.worst_phase_deviation_ns());
        self.cv.notify_all();
        waited
    }

    fn publish_status(&self, status: &LinkStatus) {
        let mut state = self.lock();
        if state.actual != Actual::Open {
            return;
        }
        let num_devices = state.num_devices;
        debug_assert_eq!(
            status.devices().len(),
            num_devices,
            "the state checker must report exactly the devices the bus was opened with",
        );
        state.status.devices.clear();
        state.status.devices.extend(
            status
                .devices()
                .iter()
                .copied()
                .chain(std::iter::repeat(DeviceState::Lost))
                .take(num_devices),
        );
        state.link_recoveries = status.recoveries();
        state.status.recoveries = state.base.recoveries + state.link_recoveries;
    }

    fn wait_while_idle(&self) {
        let state = self.lock();
        let _ = self
            .cv
            .wait_timeout(state, IDLE_WAIT_PERIOD)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }

    fn wait_backoff(&self, period: Duration) {
        let deadline = Instant::now() + period;
        let mut state = self.lock();
        loop {
            if self.is_stopped() || self.desired() == Desired::Closed {
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            state = self
                .cv
                .wait_timeout(state, remaining.min(IDLE_WAIT_PERIOD))
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    fn sleep_unless_stopped(&self, period: Duration) -> bool {
        let deadline = Instant::now() + period;
        loop {
            if self.is_stopped() {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            std::thread::sleep(remaining.min(SHUTDOWN_POLL_PERIOD));
        }
    }

    pub(crate) fn exchange(
        &self,
        num_devices: usize,
        tx: &[u8],
        rx: &mut [u8],
        status: &mut BusStatus,
    ) -> Result<FrameReply, RemoteLinkError> {
        let mut state = self.lock();
        if self.is_stopped() {
            return Err(RemoteLinkError::Link("the bus loop stopped".to_owned()));
        }
        if self.desired() == Desired::Closed {
            return Err(RemoteLinkError::BusClosed);
        }
        if state.num_devices != num_devices {
            return Err(RemoteLinkError::DeviceCountChanged {
                expected: num_devices,
                found: state.num_devices,
            });
        }
        state.tx.as_flattened_mut().copy_from_slice(tx);
        state.tx_version += 1;
        let want = state.tx_version;
        let mut recovery_deadline = None;
        loop {
            if self.is_stopped() {
                return Err(RemoteLinkError::Link("the bus loop stopped".to_owned()));
            }
            if self.desired() == Desired::Closed {
                return Err(RemoteLinkError::BusClosed);
            }
            if state.num_devices != num_devices {
                return Err(RemoteLinkError::DeviceCountChanged {
                    expected: num_devices,
                    found: state.num_devices,
                });
            }
            if state.actual == Actual::Open {
                if state.applied_version >= want {
                    rx.copy_from_slice(state.rx.as_flattened());
                    status.clone_from(&state.status);
                    return Ok(FrameReply {
                        rx_valid: state.rx_valid,
                        dc_time_ns: state.dc_time_ns,
                    });
                }
                recovery_deadline = None;
                state = self
                    .cv
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                continue;
            }
            let deadline =
                *recovery_deadline.get_or_insert_with(|| Instant::now() + RECOVERING_REPLY_PERIOD);
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                rx.copy_from_slice(state.rx.as_flattened());
                status.clone_from(&state.status);
                return Ok(FrameReply {
                    rx_valid: false,
                    dc_time_ns: wire::DC_TIME_UNAVAILABLE,
                });
            }
            state = self
                .cv
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }
}

struct OpenedLink<L> {
    link: L,
    dc_clock: Option<DcClock>,
    stats: LinkStats,
    tx: Vec<[u8; TX_FRAME_BYTES]>,
    rx: Vec<[u8; RX_FRAME_BYTES]>,
}

fn open_link<L, F>(
    shared: &BusShared,
    factory: &mut F,
    checker_tx: &Sender<L::Checker>,
) -> Option<OpenedLink<L>>
where
    L: Link,
    F: FnMut() -> Result<L, RemoteLinkError>,
{
    shared.enter_opening();
    let reason = match factory() {
        Ok(opened) if opened.num_devices() > 0 => {
            let num_devices = opened.num_devices();
            let _ = checker_tx.send(opened.state_checker());
            shared.set_stats(&opened.stats());
            shared.enter_open(num_devices);
            tracing::info!(devices = num_devices, "bus is up");
            return Some(OpenedLink {
                dc_clock: opened.dc_clock(),
                stats: opened.stats(),
                tx: vec![[0u8; TX_FRAME_BYTES]; num_devices],
                rx: vec![[0u8; RX_FRAME_BYTES]; num_devices],
                link: opened,
            });
        }
        Ok(mut empty) => {
            let _ = empty.close();
            "no device found on the bus".to_owned()
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to open the bus link; retrying");
            e.to_string()
        }
    };
    shared.enter_failed(&reason);
    shared.wait_backoff(REOPEN_RETRY_PERIOD);
    None
}

fn close_link<L: Link>(link: &mut Option<L>) -> bool {
    let Some(mut link) = link.take() else {
        return false;
    };
    if let Err(e) = link.close() {
        tracing::warn!(error = %e, "failed to close the bus link");
    }
    true
}

struct StopOnDrop<'a>(&'a BusShared);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.stop();
    }
}

fn panic_reason(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown payload".to_owned()
    }
}

pub(crate) fn run_bus_loop<L, F>(
    shared: &BusShared,
    option: &BusOption,
    factory: F,
    checker_tx: &Sender<L::Checker>,
) where
    L: Link,
    F: FnMut() -> Result<L, RemoteLinkError>,
{
    let _stop = StopOnDrop(shared);
    if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bus_loop(shared, option, factory, checker_tx);
    })) {
        let reason = format!("the bus thread panicked: {}", panic_reason(&payload));
        tracing::error!(reason, "the bus thread panicked; the bus stays down");
        shared.enter_failed(&reason);
    }
}

fn bus_loop<L, F>(
    shared: &BusShared,
    option: &BusOption,
    mut factory: F,
    checker_tx: &Sender<L::Checker>,
) where
    L: Link,
    F: FnMut() -> Result<L, RemoteLinkError>,
{
    shared.publish_tuning(autd3_rs_core::apply_thread_tuning(option.tuning()));
    prefault_stack(option.stack_prefault_bytes);

    let mut link: Option<L> = None;
    let mut dc_clock: Option<DcClock> = None;
    let mut link_stats = LinkStats::default();
    let mut tx_local: Vec<[u8; TX_FRAME_BYTES]> = Vec::new();
    let mut rx_local: Vec<[u8; RX_FRAME_BYTES]> = Vec::new();
    let mut diag = CycleDiag::default();
    let mut published: Option<Instant> = None;

    while !shared.is_stopped() {
        if shared.desired() == Desired::Closed {
            if close_link(&mut link) {
                tracing::info!("bus closed on request");
            }
            shared.enter_closed();
            published = None;
            if shared.take_probe_request() {
                let result = match factory() {
                    Ok(mut probed) => {
                        let devices = probed.num_devices();
                        let _ = probed.close();
                        Ok(devices)
                    }
                    Err(e) => Err(e.to_string()),
                };
                tracing::info!(?result, "probed the bus");
                shared.finish_probe(result);
                continue;
            }
            shared.wait_while_idle();
            continue;
        }

        let Some(active) = link.as_mut() else {
            if let Some(opened) = open_link(shared, &mut factory, checker_tx) {
                dc_clock = opened.dc_clock;
                link_stats = opened.stats;
                tx_local = opened.tx;
                rx_local = opened.rx;
                link = Some(opened.link);
                published = None;
            }
            continue;
        };

        let iter_start = Instant::now();
        let pre = published.map_or(Duration::ZERO, |at| iter_start.duration_since(at));
        active.wait_next_cycle();
        let (version, waited) = shared.take_tx(&mut tx_local);
        diag.observe_lock(waited);

        let start = Instant::now();
        let result = active.cycle(&tx_local, &mut rx_local);
        let cycled = start.elapsed();
        diag.observe_cycle(cycled);

        match result {
            Ok(outcome) => {
                let dc_time_ns = dc_clock
                    .as_ref()
                    .and_then(DcClock::now)
                    .map_or(wire::DC_TIME_UNAVAILABLE, DcSysTime::sys_time);
                let waited = shared.publish_cycle(
                    &rx_local,
                    outcome.rx_valid(),
                    dc_time_ns,
                    version,
                    &link_stats,
                );
                diag.observe_lock(waited);
                let now = Instant::now();
                let post = now.duration_since(start).saturating_sub(cycled);
                published = Some(now);
                if pre >= DIAG_SLOW_ITERATION
                    || cycled >= DIAG_SLOW_ITERATION
                    || post >= DIAG_SLOW_ITERATION
                {
                    tracing::warn!(
                        pre_us = pre.as_micros(),
                        cycle_us = cycled.as_micros(),
                        post_us = post.as_micros(),
                        "slow bus loop iteration",
                    );
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "bus cycle failed; reopening the bus link");
                close_link(&mut link);
                shared.enter_recovering();
                continue;
            }
        }

        diag.report(&link_stats);
        option.pacing.wait_after(start);
    }

    close_link(&mut link);
}

pub(crate) fn run_status_loop<C: StateCheck>(
    shared: &BusShared,
    option: &BusOption,
    checker_rx: &Receiver<C>,
) {
    autd3_rs_core::apply_thread_tuning(option.session_tuning());

    let mut checker = None;
    loop {
        if shared.sleep_unless_stopped(STATUS_SAMPLE_PERIOD) {
            shared.sample();
            return;
        }
        loop {
            match checker_rx.try_recv() {
                Ok(next) => checker = Some(next),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    shared.sample();
                    return;
                }
            }
        }
        if let Some(checker) = checker.as_mut() {
            match checker.check() {
                Ok(status) => shared.publish_status(&status),
                Err(e) => tracing::debug!(error = %e, "bus state check failed"),
            }
        }
        shared.sample();
    }
}

#[derive(Default)]
struct CycleDiag {
    cycles: u64,
    lock_max: Duration,
    lock_over_alert: u64,
    cycle_max: Duration,
    window_start: Option<Instant>,
}

impl CycleDiag {
    fn observe_lock(&mut self, waited: Duration) {
        self.lock_max = self.lock_max.max(waited);
        if waited >= DIAG_LOCK_ALERT {
            self.lock_over_alert += 1;
        }
    }

    fn observe_cycle(&mut self, elapsed: Duration) {
        self.cycles += 1;
        self.cycle_max = self.cycle_max.max(elapsed);
    }

    fn report(&mut self, stats: &LinkStats) {
        let started = *self.window_start.get_or_insert_with(Instant::now);
        if started.elapsed() < DIAG_REPORT_PERIOD {
            return;
        }
        tracing::debug!(
            cycles = self.cycles,
            lock_max_us = self.lock_max.as_micros(),
            lock_over_100us = self.lock_over_alert,
            cycle_max_us = self.cycle_max.as_micros(),
            exchange_mean_us = stats.mean_exchange_ns() / 1_000,
            exchange_worst_us = stats.worst_exchange_ns() / 1_000,
            "bus loop timing",
        );
        *self = Self {
            window_start: Some(Instant::now()),
            ..Self::default()
        };
    }
}

fn prefault_stack(bytes: usize) {
    if bytes == 0 {
        return;
    }
    prefault_frame(bytes);
    tracing::debug!(bytes, "prefaulted the bus thread stack");
}

#[inline(never)]
fn prefault_frame(remaining: usize) {
    let mut frame = [0u8; PREFAULT_FRAME_BYTES];
    for slot in frame.iter_mut().step_by(PAGE_BYTES) {
        *slot = 1;
    }
    std::hint::black_box(&mut frame);
    if remaining > PREFAULT_FRAME_BYTES {
        prefault_frame(remaining - PREFAULT_FRAME_BYTES);
    }
}

pub struct SharedBus {
    shared: Arc<BusShared>,
    option: BusOption,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl SharedBus {
    pub fn new<L, F>(option: BusOption, factory: F) -> Result<Arc<Self>, RemoteLinkError>
    where
        L: Link + 'static,
        F: FnMut() -> Result<L, RemoteLinkError> + Send + 'static,
    {
        let shared = Arc::new(BusShared::new());
        let (checker_tx, checker_rx) = std::sync::mpsc::channel();

        let mut builder = std::thread::Builder::new().name("autd3-remote-bus".to_owned());
        if option.stack_prefault_bytes > 0 {
            builder = builder.stack_size(option.stack_prefault_bytes + STACK_HEADROOM_BYTES);
        }
        let bus_shared = Arc::clone(&shared);
        let bus = builder
            .spawn(move || {
                run_bus_loop(&bus_shared, &option, factory, &checker_tx);
            })
            .map_err(|e| RemoteLinkError::Link(format!("failed to spawn bus thread: {e}")))?;

        let status_shared = Arc::clone(&shared);
        let status = std::thread::Builder::new()
            .name("autd3-remote-status".to_owned())
            .spawn(move || run_status_loop(&status_shared, &option, &checker_rx))
            .map_err(|e| RemoteLinkError::Link(format!("failed to spawn status thread: {e}")))?;

        Ok(Arc::new(Self {
            shared,
            option,
            threads: Mutex::new(vec![bus, status]),
        }))
    }

    pub fn set_desired(&self, desired: Desired) {
        self.shared.set_desired(desired);
    }

    pub fn hold(&self, reason: impl Into<String>) {
        self.shared.set_hold(Some(reason.into()));
    }

    pub fn release(&self) {
        self.shared.set_hold(None);
    }

    pub fn wait_actual(
        &self,
        timeout: Duration,
        reached: impl Fn(&Actual) -> bool,
        give_up: impl Fn() -> bool,
    ) -> bool {
        self.shared.wait_actual(timeout, reached, give_up)
    }

    #[must_use]
    pub fn applied_tuning(&self) -> Option<RtThreadTuning> {
        self.shared.wait_for_tuning(TUNING_WAIT_TIMEOUT)
    }

    #[must_use]
    pub fn snapshot(&self) -> BusSnapshot {
        self.shared.snapshot()
    }

    #[must_use]
    pub fn sampled(&self) -> Arc<BusSnapshot> {
        self.shared.sampled()
    }

    pub fn probe(&self) -> Result<usize, RemoteLinkError> {
        self.shared.request_probe()
    }

    pub fn exchange_while_held(
        &self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<bool, RemoteLinkError> {
        if self.shared.hold_reason().is_none() {
            return Err(RemoteLinkError::Link(
                "the bus is not held; take it with hold() before driving frames".to_owned(),
            ));
        }
        if tx.len() != rx.len() {
            return Err(RemoteLinkError::DeviceCountChanged {
                expected: tx.len(),
                found: rx.len(),
            });
        }
        let mut status = BusStatus::default();
        self.shared
            .exchange(
                tx.len(),
                tx.as_flattened(),
                rx.as_flattened_mut(),
                &mut status,
            )
            .map(|reply| reply.rx_valid)
    }

    pub fn shutdown(&self) {
        self.shared.stop();
        for thread in std::mem::take(
            &mut *self
                .threads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        ) {
            let _ = thread.join();
        }
    }

    pub(crate) fn shared(&self) -> &Arc<BusShared> {
        &self.shared
    }

    pub(crate) fn option(&self) -> &BusOption {
        &self.option
    }
}

impl Drop for SharedBus {
    fn drop(&mut self) {
        self.shutdown();
    }
}
