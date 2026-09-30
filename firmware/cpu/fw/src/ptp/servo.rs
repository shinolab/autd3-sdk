pub const STEP_THRESHOLD_NS: i64 = 10_000;
pub const LOCK_THRESHOLD_NS: i64 = 100;
pub const LOCK_SAMPLES: u32 = 64;
pub const KP_MILLI: i64 = 100;
pub const KI_MILLI: i64 = 20;
pub const MAX_FREQ_PPB: i64 = 500_000;
pub const DEFAULT_INTERVAL_NS: i64 = 16_000_000;
const MAX_SAMPLE_GAP_NS: i64 = 1_000_000_000;
const MILLI: i64 = 1000;
const NS_PER_SEC: i128 = 1_000_000_000;
const MAX_FREQ_MPPB: i64 = MAX_FREQ_PPB * MILLI;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Fresh,
    Estimating { offset: i64, at: i64 },
    Tracking,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Output {
    pub step_ns: Option<i64>,
    pub drift_ppb: Option<i32>,
    pub locked_now: bool,
    pub unlocked: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Servo {
    state: State,
    integral_mppb: i64,
    last_at: Option<i64>,
    lock_count: u32,
    locked: bool,
}

impl Default for Servo {
    fn default() -> Self {
        Self::new()
    }
}

fn clamp_mppb(v: i128) -> i64 {
    v.clamp(i128::from(-MAX_FREQ_MPPB), i128::from(MAX_FREQ_MPPB)) as i64
}

fn round_ppb(mppb: i64) -> i32 {
    let half = if mppb >= 0 { MILLI / 2 } else { -MILLI / 2 };
    ((mppb + half) / MILLI) as i32
}

impl Servo {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: State::Fresh,
            integral_mppb: 0,
            last_at: None,
            lock_count: 0,
            locked: false,
        }
    }

    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn unlock(&mut self) {
        self.locked = false;
        self.lock_count = 0;
    }

    pub fn sample(&mut self, offset: i64, at: i64) -> Output {
        match self.state {
            State::Fresh => {
                if offset.abs() > STEP_THRESHOLD_NS {
                    self.state = State::Estimating { offset, at };
                    return Output::default();
                }
                self.state = State::Tracking;
                self.track(offset, at)
            }
            State::Estimating {
                offset: first,
                at: first_at,
            } => {
                let mut out = self.step(offset);
                let elapsed = at - first_at;
                if elapsed > 0 {
                    let excess = i128::from(offset - first) * NS_PER_SEC * i128::from(MILLI)
                        / i128::from(elapsed);
                    self.integral_mppb = clamp_mppb(-excess);
                    out.drift_ppb = Some(round_ppb(self.integral_mppb));
                }
                self.state = State::Tracking;
                out
            }
            State::Tracking => {
                if offset.abs() > STEP_THRESHOLD_NS {
                    return self.step(offset);
                }
                self.track(offset, at)
            }
        }
    }

    fn step(&mut self, offset: i64) -> Output {
        let was_locked = self.locked;
        self.locked = false;
        self.lock_count = 0;
        self.last_at = None;
        Output {
            step_ns: Some(-offset),
            drift_ppb: None,
            locked_now: false,
            unlocked: was_locked,
        }
    }

    fn track(&mut self, offset: i64, at: i64) -> Output {
        let elapsed = self
            .last_at
            .map(|prev| at - prev)
            .filter(|&gap| gap > 0 && gap < MAX_SAMPLE_GAP_NS)
            .unwrap_or(DEFAULT_INTERVAL_NS);
        self.last_at = Some(at);
        let rate_mppb = i128::from(offset) * NS_PER_SEC * i128::from(MILLI) / i128::from(elapsed);
        self.integral_mppb = clamp_mppb(
            i128::from(self.integral_mppb) - i128::from(KI_MILLI) * rate_mppb / i128::from(MILLI),
        );
        let mppb = clamp_mppb(
            i128::from(self.integral_mppb) - i128::from(KP_MILLI) * rate_mppb / i128::from(MILLI),
        );
        if offset.abs() < LOCK_THRESHOLD_NS {
            self.lock_count = self.lock_count.saturating_add(1);
        } else {
            self.lock_count = 0;
        }
        let locked_now = !self.locked && self.lock_count >= LOCK_SAMPLES;
        if locked_now {
            self.locked = true;
        }
        Output {
            step_ns: None,
            drift_ppb: Some(round_ppb(mppb)),
            locked_now,
            unlocked: false,
        }
    }
}
