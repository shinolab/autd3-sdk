#![allow(clippy::cast_possible_truncation)]

use core::time::Duration;

use crate::Freq;
use crate::value::{Nearest, SamplingConfig};

#[derive(Clone, Copy, Debug, PartialEq)]
enum StmConfigInner {
    Freq(Freq<f32>),
    Period(Duration),
    Sampling(SamplingConfig),
    FreqNearest(Freq<f32>),
    PeriodNearest(Duration),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StmConfig(StmConfigInner);

impl StmConfig {
    #[must_use]
    pub fn into_sampling_config(self, size: usize) -> SamplingConfig {
        let size = size.max(1);
        match self.0 {
            StmConfigInner::Freq(freq) => SamplingConfig::new(freq * size as f32),
            StmConfigInner::Period(period) => SamplingConfig::new(period / size as u32),
            StmConfigInner::Sampling(config) => config,
            StmConfigInner::FreqNearest(freq) => SamplingConfig::new(Nearest(freq * size as f32)),
            StmConfigInner::PeriodNearest(period) => {
                SamplingConfig::new(Nearest(period / size as u32))
            }
        }
    }
}

impl From<Freq<f32>> for StmConfig {
    fn from(freq: Freq<f32>) -> Self {
        Self(StmConfigInner::Freq(freq))
    }
}

impl From<Duration> for StmConfig {
    fn from(period: Duration) -> Self {
        Self(StmConfigInner::Period(period))
    }
}

impl From<SamplingConfig> for StmConfig {
    fn from(config: SamplingConfig) -> Self {
        Self(StmConfigInner::Sampling(config))
    }
}

impl From<Nearest<Freq<f32>>> for StmConfig {
    fn from(freq: Nearest<Freq<f32>>) -> Self {
        Self(StmConfigInner::FreqNearest(freq.0))
    }
}

impl From<Nearest<Duration>> for StmConfig {
    fn from(period: Nearest<Duration>) -> Self {
        Self(StmConfigInner::PeriodNearest(period.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Hz;

    #[test]
    fn stm_freq_multiplies_sampling_rate_by_size() {
        assert_eq!(
            StmConfig::from(100.0 * Hz).into_sampling_config(4).divide(),
            Ok(100)
        );
    }

    #[test]
    fn stm_period_divides_by_size() {
        assert_eq!(
            StmConfig::from(Duration::from_millis(1))
                .into_sampling_config(4)
                .divide(),
            Ok(10)
        );
    }

    #[test]
    fn stm_sampling_config_passes_through_regardless_of_size() {
        assert_eq!(
            StmConfig::from(SamplingConfig::FREQ_4K)
                .into_sampling_config(7)
                .divide(),
            Ok(10)
        );
    }

    #[test]
    fn stm_nearest_freq_rounds_to_a_valid_divider() {
        assert!(
            StmConfig::from(Nearest(4001.0 * Hz))
                .into_sampling_config(1)
                .divide()
                .is_ok()
        );
    }

    #[test]
    fn stm_nearest_period_rounds_to_a_valid_divider() {
        assert!(
            StmConfig::from(Nearest(Duration::from_micros(251)))
                .into_sampling_config(1)
                .divide()
                .is_ok()
        );
    }
}
