use core::num::NonZeroU16;

use autd3_rs_core::Freq;
use autd3_rs_core::params::ULTRASOUND_FREQ_HZ;

#[must_use]
pub fn samples_per_period(divider: NonZeroU16, freq: Freq<u32>) -> Option<u32> {
    let divider = u32::from(divider.get());
    let freq_hz = freq.hz();
    if freq_hz == 0 || !ULTRASOUND_FREQ_HZ.is_multiple_of(divider) {
        return None;
    }
    let fs = ULTRASOUND_FREQ_HZ / divider;
    fs.is_multiple_of(freq_hz).then(|| fs / freq_hz)
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
        assert_eq!(samples_per_period(div(10), 300 * Hz), None);
        assert_eq!(samples_per_period(div(10), 0 * Hz), None);
        assert_eq!(samples_per_period(div(3), 100 * Hz), None);
    }
}
