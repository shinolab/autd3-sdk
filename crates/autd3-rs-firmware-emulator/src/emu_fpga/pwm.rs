use autd3_rs_core::value::PULSE_WIDTH_PERIOD;

pub fn pwm_levels(pulse_width: u16, phase: u8) -> impl Iterator<Item = bool> {
    let phase = u16::from(phase) * 2;
    let rise = (PULSE_WIDTH_PERIOD + phase - pulse_width / 2) % PULSE_WIDTH_PERIOD;
    let fall = (phase + pulse_width / 2 + (pulse_width & 0x01)) % PULSE_WIDTH_PERIOD;
    (0..PULSE_WIDTH_PERIOD).map(move |i| {
        if rise <= fall {
            rise <= i && i < fall
        } else {
            i < fall || rise <= i
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn high_samples(pulse_width: u16, phase: u8) -> Vec<u16> {
        pwm_levels(pulse_width, phase)
            .zip(0..)
            .filter_map(|(high, i)| high.then_some(i))
            .collect()
    }

    #[test]
    fn a_period_has_one_sample_per_pulse_width_step() {
        assert_eq!(pwm_levels(256, 0).count(), usize::from(PULSE_WIDTH_PERIOD));
    }

    #[test]
    fn the_pulse_is_centred_on_twice_the_phase() {
        assert_eq!(high_samples(4, 100), (198..202).collect::<Vec<_>>());
        assert_eq!(high_samples(5, 100), (198..203).collect::<Vec<_>>());
    }

    #[test]
    fn the_pulse_wraps_around_the_period_boundary() {
        assert_eq!(
            high_samples(256, 0),
            (0..128).chain(384..512).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_zero_pulse_width_stays_low() {
        assert!(pwm_levels(0, 37).all(|high| !high));
    }
}
