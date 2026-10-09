use super::Config;

const MAX_SAMPLE_GAP_NS: i64 = 1_000_000_000;
const MILLI: i64 = 1000;
const NS_PER_SEC: i128 = 1_000_000_000;

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

fn clamp_mppb(v: i128, config: &Config) -> i64 {
    let max = i128::from(config.max_freq_ppb) * i128::from(MILLI);
    v.clamp(-max, max) as i64
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

    pub fn unlock(&mut self) {
        self.locked = false;
        self.lock_count = 0;
    }

    pub fn sample(&mut self, offset: i64, at: i64, config: &Config) -> Output {
        match self.state {
            State::Fresh => {
                if offset.abs() > config.step_threshold.as_nanos() as i64 {
                    self.state = State::Estimating { offset, at };
                    return Output::default();
                }
                self.state = State::Tracking;
                self.track(offset, at, config)
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
                    self.integral_mppb = clamp_mppb(-excess, config);
                    out.drift_ppb = Some(round_ppb(self.integral_mppb));
                }
                self.state = State::Tracking;
                out
            }
            State::Tracking => {
                if offset.abs() > config.step_threshold.as_nanos() as i64 {
                    return self.step(offset);
                }
                self.track(offset, at, config)
            }
        }
    }

    fn step(&mut self, offset: i64) -> Output {
        self.locked = false;
        self.lock_count = 0;
        self.last_at = None;
        Output {
            step_ns: Some(-offset),
            drift_ppb: None,
            locked_now: false,
        }
    }

    fn track(&mut self, offset: i64, at: i64, config: &Config) -> Output {
        let sync_interval = config.sync_interval.as_nanos() as i64;
        let elapsed = self
            .last_at
            .map(|prev| at - prev)
            .filter(|&gap| gap > 0 && gap < MAX_SAMPLE_GAP_NS)
            .map_or(sync_interval, |gap| gap.max(sync_interval));
        self.last_at = Some(at);
        let rate_mppb = i128::from(offset) * NS_PER_SEC * i128::from(MILLI) / i128::from(elapsed);
        self.integral_mppb = clamp_mppb(
            i128::from(self.integral_mppb)
                - i128::from(config.ki_milli) * rate_mppb / i128::from(MILLI),
            config,
        );
        let mppb = clamp_mppb(
            i128::from(self.integral_mppb)
                - i128::from(config.kp_milli) * rate_mppb / i128::from(MILLI),
            config,
        );
        if offset.abs() < config.lock_threshold.as_nanos() as i64 {
            self.lock_count = self.lock_count.saturating_add(1);
        } else {
            self.lock_count = 0;
        }
        let locked_now = !self.locked && self.lock_count >= u32::from(config.lock_samples.get());
        if locked_now {
            self.locked = true;
        }
        Output {
            step_ns: None,
            drift_ppb: Some(round_ppb(mppb)),
            locked_now,
        }
    }
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU16;
    use core::time::Duration;

    use super::{Config, Servo};

    const LOCK_SAMPLES: u16 = autd3_cpu_wire::cpu_params::PTP_LOCK_SAMPLES.get();
    const STEP_THRESHOLD_NS: i64 = autd3_cpu_wire::cpu_params::PTP_STEP_THRESHOLD.as_nanos() as i64;

    #[test]
    fn the_servo_needs_64_quiet_samples_to_lock() {
        let mut servo = Servo::new();
        let mut at = 0i64;
        for i in 0..LOCK_SAMPLES {
            at += 16_000_000;
            let out = servo.sample(50, at, &Config::default());
            assert_eq!(out.locked_now, i + 1 == LOCK_SAMPLES);
            assert!(out.step_ns.is_none());
        }
        at += 16_000_000;
        assert!(!servo.sample(50, at, &Config::default()).locked_now);
        at += 16_000_000;
        let out = servo.sample(STEP_THRESHOLD_NS + 1, at, &Config::default());
        assert_eq!(out.step_ns, Some(-(STEP_THRESHOLD_NS + 1)));
        assert!(!out.locked_now);
        for i in 0..LOCK_SAMPLES {
            at += 16_000_000;
            assert_eq!(
                servo.sample(50, at, &Config::default()).locked_now,
                i + 1 == LOCK_SAMPLES
            );
        }
        servo.unlock();
        for i in 0..LOCK_SAMPLES {
            at += 16_000_000;
            assert_eq!(
                servo.sample(50, at, &Config::default()).locked_now,
                i + 1 == LOCK_SAMPLES
            );
        }
    }

    #[test]
    fn the_servo_estimates_the_frequency_before_its_first_step() {
        let mut servo = Servo::new();
        assert_eq!(
            servo.sample(1_000_000, 0, &Config::default()),
            super::Output::default()
        );
        let out = servo.sample(1_016_000, 16_000_000, &Config::default());
        assert_eq!(out.step_ns, Some(-1_016_000));
        assert_eq!(out.drift_ppb, Some(-500_000));
        let mut servo = Servo::new();
        servo.sample(1_000_000, 0, &Config::default());
        let out = servo.sample(1_000_160, 16_000_000, &Config::default());
        assert_eq!(out.drift_ppb, Some(-10_000));
    }

    #[test]
    fn a_moderate_offset_pulls_the_frequency_proportionally() {
        let mut servo = Servo::new();
        let out = servo.sample(1_600, 8_000_000, &Config::default());
        assert_eq!(out.drift_ppb, Some(-10_200));
    }

    #[test]
    fn the_lock_follows_the_configured_threshold_and_sample_count() {
        let config = Config {
            lock_samples: NonZeroU16::new(3).unwrap(),
            lock_threshold: Duration::from_nanos(20),
            ..Config::default()
        };
        let mut servo = Servo::new();
        let mut at = 0i64;
        for _ in 0..8 {
            at += 16_000_000;
            assert!(!servo.sample(50, at, &config).locked_now);
        }
        for i in 0..3 {
            at += 16_000_000;
            assert_eq!(servo.sample(19, at, &config).locked_now, i == 2);
        }
    }

    #[test]
    fn the_step_follows_the_configured_threshold() {
        let config = Config {
            step_threshold: Duration::from_micros(1),
            ..Config::default()
        };
        let mut servo = Servo::new();
        assert!(servo.sample(1_000, 16_000_000, &config).step_ns.is_none());
        assert_eq!(
            servo.sample(1_001, 32_000_000, &config).step_ns,
            Some(-1_001)
        );
    }

    #[test]
    fn the_gains_and_the_frequency_limit_follow_the_config() {
        let mut servo = Servo::new();
        let config = Config {
            kp_milli: 200,
            ki_milli: 0,
            ..Config::default()
        };
        assert_eq!(
            servo.sample(1_600, 16_000_000, &config).drift_ppb,
            Some(-40_000)
        );

        let mut servo = Servo::new();
        let config = Config {
            max_freq_ppb: 5_000,
            ..Config::default()
        };
        assert_eq!(
            servo.sample(1_600, 16_000_000, &config).drift_ppb,
            Some(-5_000)
        );
    }

    #[test]
    fn samples_closer_than_the_sync_interval_count_as_one_interval_apart() {
        let config = Config::default();
        let drift_after = |gap_ns: i64| {
            let mut servo = Servo::new();
            let _ = servo.sample(40, 16_000_000, &config);
            servo.sample(40, 16_000_000 + gap_ns, &config).drift_ppb
        };
        assert_eq!(drift_after(20_000), drift_after(8_000_000));
        assert_ne!(drift_after(16_000_000), drift_after(8_000_000));
    }

    #[test]
    fn the_first_sample_interval_comes_from_the_sync_interval() {
        let mut servo = Servo::new();
        let config = Config {
            sync_interval: Duration::from_millis(32),
            ..Config::default()
        };
        assert_eq!(
            servo.sample(1_600, 16_000_000, &config).drift_ppb,
            Some(-2_550)
        );
    }
}
