use std::time::Duration;

use crate::cli::TuneArgs;
use crate::monitor::{CandidateResult, CandidateStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub period: Duration,
    pub shift_percent: u8,
}

impl Candidate {
    #[must_use]
    pub fn shift(self) -> Duration {
        shift_duration(self.period, self.shift_percent)
    }
}

#[must_use]
pub fn shift_duration(period: Duration, percent: u8) -> Duration {
    let nanos = period.as_nanos() * u128::from(percent) / 100;
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

#[must_use]
pub fn inclusive_range(min: u64, max: u64, step: u64) -> Vec<u64> {
    if step == 0 || min > max {
        return vec![min];
    }
    let mut out = Vec::new();
    let mut x = min;
    while x <= max {
        out.push(x);
        x += step;
    }
    out
}

#[must_use]
pub fn candidates(args: &TuneArgs) -> Vec<Candidate> {
    let periods = inclusive_range(args.period_min, args.period_max, args.period_step);
    let shifts = inclusive_range(
        u64::from(args.shift_min),
        u64::from(args.shift_max),
        u64::from(args.shift_step),
    );
    let mut out = Vec::with_capacity(periods.len() * shifts.len());
    for &p in &periods {
        for &s in &shifts {
            out.push(Candidate {
                period: Duration::from_micros(p),
                shift_percent: u8::try_from(s).unwrap_or(100),
            });
        }
    }
    out
}

#[must_use]
pub fn select_best(results: &[CandidateResult]) -> Option<usize> {
    results
        .iter()
        .enumerate()
        .filter(|(_, r)| r.status == CandidateStatus::Ok && r.total_samples > 0)
        .max_by(|(_, a), (_, b)| {
            a.op_ratio()
                .total_cmp(&b.op_ratio())
                .then(b.drop_events.cmp(&a.drop_events))
                .then(b.shift.cmp(&a.shift))
                .then(b.period.cmp(&a.period))
        })
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inclusive_range_basic() {
        assert_eq!(inclusive_range(500, 2000, 500), vec![500, 1000, 1500, 2000]);
        assert_eq!(inclusive_range(0, 100, 25), vec![0, 25, 50, 75, 100]);
    }

    #[test]
    fn inclusive_range_degenerate() {
        assert_eq!(inclusive_range(1000, 1000, 500), vec![1000]);
        assert_eq!(inclusive_range(1000, 2000, 0), vec![1000]);
    }

    #[test]
    fn shift_is_fraction_of_period() {
        assert_eq!(
            shift_duration(Duration::from_millis(2), 100),
            Duration::from_millis(2)
        );
        assert_eq!(
            shift_duration(Duration::from_millis(2), 50),
            Duration::from_millis(1)
        );
        assert_eq!(shift_duration(Duration::from_millis(1), 0), Duration::ZERO);
        assert_eq!(
            shift_duration(Duration::from_micros(1500), 25),
            Duration::from_micros(375)
        );
    }
}
