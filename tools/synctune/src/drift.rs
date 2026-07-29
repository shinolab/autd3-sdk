use std::time::Duration;

const MAX_SAMPLES: usize = 1_000_000;

#[derive(Debug, Clone, Default)]
pub struct DriftStats {
    pub samples: usize,
    pub window: Duration,
    pub first_offset_ns: f64,
    pub last_offset_ns: f64,
    pub rate_ppm: Option<f64>,
    pub residual_rms_ns: f64,
}

impl DriftStats {
    #[must_use]
    pub fn time_to(&self, budget: Duration) -> Option<Duration> {
        let rate_ppm = self.rate_ppm?;
        let ns_per_s = rate_ppm.abs() * 1e3;
        if ns_per_s <= 0.0 {
            return None;
        }
        let secs = budget.as_secs_f64() * 1e9 / ns_per_s;
        Duration::try_from_secs_f64(secs).ok()
    }
}

#[derive(Debug, Default)]
pub struct DriftAccumulator {
    samples: Vec<(f64, f64)>,
}

impl DriftAccumulator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, elapsed: Duration, offset_ns: i64) {
        if self.samples.len() >= MAX_SAMPLES {
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        self.samples.push((elapsed.as_secs_f64(), offset_ns as f64));
    }

    #[must_use]
    pub fn finish(self) -> DriftStats {
        let n = self.samples.len();
        let Some((&(first_x, first_y), &(last_x, last_y))) =
            self.samples.first().zip(self.samples.last())
        else {
            return DriftStats::default();
        };
        let mut stats = DriftStats {
            samples: n,
            window: Duration::try_from_secs_f64(last_x - first_x).unwrap_or_default(),
            first_offset_ns: first_y,
            last_offset_ns: last_y,
            rate_ppm: None,
            residual_rms_ns: 0.0,
        };
        if n < 2 {
            return stats;
        }

        #[allow(clippy::cast_precision_loss)]
        let count = n as f64;
        let mean_x = self.samples.iter().map(|(x, _)| x).sum::<f64>() / count;
        let mean_y = self.samples.iter().map(|(_, y)| y).sum::<f64>() / count;
        let (sxx, sxy) = self.samples.iter().fold((0.0, 0.0), |(sxx, sxy), (x, y)| {
            let dx = x - mean_x;
            (dx.mul_add(dx, sxx), dx.mul_add(y - mean_y, sxy))
        });
        if sxx <= 0.0 {
            return stats;
        }

        let slope_ns_per_s = sxy / sxx;
        let intercept = mean_y - slope_ns_per_s * mean_x;
        let sse = self
            .samples
            .iter()
            .map(|(x, y)| {
                let residual = y - slope_ns_per_s.mul_add(*x, intercept);
                residual * residual
            })
            .sum::<f64>();
        stats.rate_ppm = Some(slope_ns_per_s / 1e3);
        stats.residual_rms_ns = (sse / count).sqrt();
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_run_reports_no_rate() {
        assert_eq!(DriftAccumulator::new().finish().rate_ppm, None);
        let mut acc = DriftAccumulator::new();
        acc.observe(Duration::ZERO, 42);
        let stats = acc.finish();
        assert_eq!(stats.samples, 1);
        assert_eq!(stats.rate_ppm, None);
    }

    #[test]
    fn a_perfect_ramp_recovers_its_rate() {
        // 25 ppm = 25 us per second = 25_000 ns per second.
        let mut acc = DriftAccumulator::new();
        for i in 0u64..100 {
            acc.observe(
                Duration::from_millis(i * 100),
                i.cast_signed() * 2_500 + 1_000_000,
            );
        }
        let stats = acc.finish();
        assert_eq!(stats.samples, 100);
        let rate = stats.rate_ppm.expect("two or more samples");
        assert!((rate - 25.0).abs() < 1e-6, "rate {rate} is not 25 ppm");
        assert!(stats.residual_rms_ns < 1e-6);
        assert!((stats.first_offset_ns - 1_000_000.0).abs() < 1e-6);
        assert!((stats.last_offset_ns - (1_000_000.0 + 99.0 * 2_500.0)).abs() < 1e-6);
    }

    #[test]
    fn extrapolation_is_the_budget_over_the_rate() {
        let stats = DriftStats {
            rate_ppm: Some(-10.0),
            ..DriftStats::default()
        };
        // 10 ppm drifts 10 ms in 1000 s and 1 s in 100_000 s.
        assert_eq!(
            stats.time_to(Duration::from_millis(10)),
            Some(Duration::from_secs(1_000))
        );
        assert_eq!(
            stats.time_to(Duration::from_secs(1)),
            Some(Duration::from_secs(100_000))
        );
        assert_eq!(DriftStats::default().time_to(Duration::from_secs(1)), None);
        assert_eq!(
            DriftStats {
                rate_ppm: Some(0.0),
                ..DriftStats::default()
            }
            .time_to(Duration::from_secs(1)),
            None
        );
    }
}
