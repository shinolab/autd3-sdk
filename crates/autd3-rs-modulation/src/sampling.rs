use autd3_rs_core::params::ULTRASOUND_FREQ_HZ;

#[must_use]
pub fn samples_per_period(divider: u16, freq_hz: u32) -> Option<u32> {
    if divider == 0 || freq_hz == 0 || !ULTRASOUND_FREQ_HZ.is_multiple_of(u32::from(divider)) {
        return None;
    }
    let fs = ULTRASOUND_FREQ_HZ / u32::from(divider);
    fs.is_multiple_of(freq_hz).then(|| fs / freq_hz)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_per_period_divides_sampling_rate() {
        assert_eq!(samples_per_period(10, 200), Some(20));
        assert_eq!(samples_per_period(1, 200), Some(200));
        assert_eq!(samples_per_period(10, 300), None);
        assert_eq!(samples_per_period(0, 200), None);
        assert_eq!(samples_per_period(10, 0), None);
        assert_eq!(samples_per_period(3, 100), None);
    }
}
