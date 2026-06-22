use std::time::Duration;

use autd3_rs::{DeviceState, LinkStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateStatus {
    Ok,

    FailedOpen,

    Aborted,
}

impl CandidateStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            CandidateStatus::Ok => "ok",
            CandidateStatus::FailedOpen => "failed-open",
            CandidateStatus::Aborted => "aborted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CandidateResult {
    pub period: Duration,
    pub shift: Duration,
    pub shift_percent: u8,
    pub status: CandidateStatus,
    pub note: Option<String>,
    pub total_samples: u64,
    pub op_all_samples: u64,

    pub safe_op_samples: u64,
    pub safe_op_error_samples: u64,
    pub lost_samples: u64,
    pub other_samples: u64,

    pub drop_events: u64,

    pub lost_events: u64,

    pub recoveries: u64,
    pub time_to_first_drop: Option<Duration>,
    pub send_success: u64,
    pub send_errors: u64,
    pub load: LoadStats,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LoadStats {
    pub send_success: u64,
    pub send_errors: u64,
    pub window: Duration,
    pub success_in_window: u64,
}

impl LoadStats {
    #[must_use]
    pub fn throughput_fps(&self) -> f64 {
        if self.window.is_zero() {
            0.0
        } else {
            self.success_in_window as f64 / self.window.as_secs_f64()
        }
    }
}

impl CandidateResult {
    #[must_use]
    pub fn new(period: Duration, shift: Duration, shift_percent: u8) -> Self {
        Self {
            period,
            shift,
            shift_percent,
            status: CandidateStatus::Ok,
            note: None,
            total_samples: 0,
            op_all_samples: 0,
            safe_op_samples: 0,
            safe_op_error_samples: 0,
            lost_samples: 0,
            other_samples: 0,
            drop_events: 0,
            lost_events: 0,
            recoveries: 0,
            time_to_first_drop: None,
            send_success: 0,
            send_errors: 0,
            load: LoadStats::default(),
        }
    }

    #[must_use]
    pub fn failed(
        period: Duration,
        shift: Duration,
        shift_percent: u8,
        status: CandidateStatus,
        note: String,
    ) -> Self {
        let mut r = Self::new(period, shift, shift_percent);
        r.status = status;
        r.note = Some(note);
        r
    }

    #[must_use]
    pub fn op_ratio(&self) -> f64 {
        if self.total_samples == 0 {
            0.0
        } else {
            self.op_all_samples as f64 / self.total_samples as f64
        }
    }
}

pub struct OpAccumulator {
    warmup: Duration,
    prev_all_op: bool,
    prev_any_lost: bool,
    seen_measured: bool,
    total_samples: u64,
    op_all_samples: u64,
    safe_op_samples: u64,
    safe_op_error_samples: u64,
    lost_samples: u64,
    other_samples: u64,
    drop_events: u64,
    lost_events: u64,
    recoveries: u64,
    time_to_first_drop: Option<Duration>,
}

impl OpAccumulator {
    #[must_use]
    pub fn new(warmup: Duration) -> Self {
        Self {
            warmup,
            prev_all_op: true,
            prev_any_lost: false,
            seen_measured: false,
            total_samples: 0,
            op_all_samples: 0,
            safe_op_samples: 0,
            safe_op_error_samples: 0,
            lost_samples: 0,
            other_samples: 0,
            drop_events: 0,
            lost_events: 0,
            recoveries: 0,
            time_to_first_drop: None,
        }
    }

    pub fn observe(&mut self, status: &LinkStatus, elapsed: Duration) {
        self.recoveries = status.recoveries;
        let all_op = status.all_op();
        let any_lost = status.any_lost();

        if elapsed < self.warmup {
            self.prev_all_op = all_op;
            self.prev_any_lost = any_lost;
            return;
        }

        if !self.seen_measured {
            self.seen_measured = true;
            self.prev_all_op = all_op;
            self.prev_any_lost = any_lost;
        }

        self.total_samples += 1;
        let since_warmup = elapsed.saturating_sub(self.warmup);
        if all_op {
            self.op_all_samples += 1;
        } else {
            self.classify(status);
            if self.prev_all_op {
                self.drop_events += 1;
                if self.time_to_first_drop.is_none() {
                    self.time_to_first_drop = Some(since_warmup);
                }
            }
        }
        if any_lost && !self.prev_any_lost {
            self.lost_events += 1;
        }

        self.prev_all_op = all_op;
        self.prev_any_lost = any_lost;
    }

    fn classify(&mut self, status: &LinkStatus) {
        let mut safe_op = false;
        let mut safe_op_error = false;
        let mut lost = false;
        let mut other = false;
        for d in &status.devices {
            match d {
                DeviceState::Op => {}
                DeviceState::SafeOp => safe_op = true,
                DeviceState::SafeOpError => safe_op_error = true,
                DeviceState::Lost => lost = true,
                DeviceState::Other(_) => other = true,
            }
        }
        if lost {
            self.lost_samples += 1;
        } else if safe_op_error {
            self.safe_op_error_samples += 1;
        } else if safe_op {
            self.safe_op_samples += 1;
        } else if other {
            self.other_samples += 1;
        }
    }

    #[must_use]
    pub fn into_result(self, mut result: CandidateResult) -> CandidateResult {
        result.total_samples = self.total_samples;
        result.op_all_samples = self.op_all_samples;
        result.safe_op_samples = self.safe_op_samples;
        result.safe_op_error_samples = self.safe_op_error_samples;
        result.lost_samples = self.lost_samples;
        result.other_samples = self.other_samples;
        result.drop_events = self.drop_events;
        result.lost_events = self.lost_events;
        result.recoveries = self.recoveries;
        result.time_to_first_drop = self.time_to_first_drop;
        result
    }
}
