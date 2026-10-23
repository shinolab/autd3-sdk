#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use core::num::NonZeroU16;

use autd3_rs_core::Freq;
use autd3_rs_core::params::ULTRASOUND_FREQ_HZ;
use autd3_rs_core::value::is_integer;

#[must_use]
pub fn samples_per_period(divider: NonZeroU16, freq: Freq) -> Option<u32> {
    let divider = u32::from(divider.get());
    if !ULTRASOUND_FREQ_HZ.is_multiple_of(divider) {
        return None;
    }
    let samples = f64::from(ULTRASOUND_FREQ_HZ / divider) / f64::from(freq.hz());
    (samples.is_finite() && (1.0..=f64::from(u32::MAX)).contains(&samples) && is_integer(samples))
        .then(|| samples.round() as u32)
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::units::Hz;

    use super::*;

    #[test]
    fn samples_per_period_divides_sampling_rate() {
        let div = |v| NonZeroU16::new(v).unwrap();
        assert_eq!(samples_per_period(div(10), 200 * Hz), Some(20));
        assert_eq!(samples_per_period(div(1), 200 * Hz), Some(200));
        assert_eq!(samples_per_period(div(10), 0.5 * Hz), Some(8000));
        assert_eq!(samples_per_period(div(10), 300 * Hz), None);
        assert_eq!(samples_per_period(div(10), 0.1 * Hz), None);
        assert_eq!(samples_per_period(div(10), 0 * Hz), None);
        assert_eq!(samples_per_period(div(10), -200.0 * Hz), None);
        assert_eq!(samples_per_period(div(10), f32::NAN * Hz), None);
        assert_eq!(samples_per_period(div(3), 100 * Hz), None);
    }
}
