use autd3_rs_core::common::{Length, Velocity};
use autd3_rs_core::params::ULTRASOUND_FREQ_HZ;

#[must_use]
pub fn wavelength(sound_speed: Velocity) -> Length {
    let freq = ULTRASOUND_FREQ_HZ as f32;
    Length::millimeters(sound_speed.mm_per_s() / freq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use autd3_rs_core::units::{m, s};

    #[test]
    fn wavelength_in_air_is_8_5_mm() {
        assert!((wavelength(340.0 * m / s).mm() - 8.5).abs() < 1e-4);
    }
}
